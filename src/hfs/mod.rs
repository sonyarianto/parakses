pub mod attribute;
pub mod btree;
pub mod catalog;
pub mod compression;
pub mod extents;
pub mod fork;
pub mod unicode;
pub mod volume_header;

use crate::blockio::BlockDevice;
use btree::BTreeReader;
use catalog::CatalogReader;
use extents::ExtentsOverflowReader;
use fork::ForkReader;
use unicode::normalize_hfs_name;
use volume_header::{HfsPlusExtentDescriptor, HfsPlusForkData, VolumeHeader};

#[derive(Debug)]
pub struct VolumeInfo {
    pub signature: u16,
    pub version: u16,
    pub volume_name: String,
    pub block_size: u32,
    pub total_blocks: u32,
    pub free_blocks: u32,
    pub file_count: u32,
    pub folder_count: u32,
    pub is_hfsx: bool,
    pub is_journaled: bool,
    pub journal_dirty: bool,
    pub write_count: u32,
}

#[derive(Debug)]
pub struct DirEntry {
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
}

pub struct HfsVolume {
    device: Box<dyn BlockDevice>,
    volume_offset: u64,
    header: VolumeHeader,
}

impl HfsVolume {
    pub fn open(device: Box<dyn BlockDevice>, volume_offset: u64) -> anyhow::Result<Self> {
        let sector_size = u64::from(device.sector_size());
        let abs_offset = volume_offset + 1024;
        let sector = abs_offset / sector_size;
        let sector_off = abs_offset % sector_size;

        let data = device.read_sector(sector)?;
        if (sector_off as usize) + 512 > data.len() {
            anyhow::bail!(
                "Volume header at byte {} crosses sector boundary",
                abs_offset
            );
        }

        let header_data = &data[sector_off as usize..sector_off as usize + 512];
        let header = VolumeHeader::parse(header_data)?;

        Ok(Self {
            device,
            volume_offset,
            header,
        })
    }

    pub fn header(&self) -> &VolumeHeader {
        &self.header
    }

    pub fn volume_info(&self) -> VolumeInfo {
        let (is_journaled, journal_dirty) = if self.header.is_journaled() {
            (true, self.check_journal_dirty())
        } else {
            (false, false)
        };

        VolumeInfo {
            signature: self.header.signature,
            version: self.header.version,
            volume_name: self.header.volume_name().to_string(),
            block_size: self.header.block_size,
            total_blocks: self.header.total_blocks,
            free_blocks: self.header.free_blocks,
            file_count: self.header.file_count,
            folder_count: self.header.folder_count,
            is_hfsx: self.header.is_hfsx(),
            is_journaled,
            journal_dirty,
            write_count: self.header.write_count,
        }
    }

    /// Read the journal header and determine if the journal is dirty.
    /// The journal_info_block is the allocation block number of the journal header.
    /// At offset 0 of that block: uint32_t flags (bit 1 = "in use" = dirty).
    fn check_journal_dirty(&self) -> bool {
        let journal_lba = u64::from(self.header.journal_info_block)
            * u64::from(self.header.block_size)
            + self.volume_offset;
        let sector_size = u64::from(self.device.as_ref().sector_size());
        let sector = journal_lba / sector_size;

        match self.device.as_ref().read_sector(sector) {
            Ok(data) => {
                let offset = (journal_lba % sector_size) as usize;
                if offset + 4 <= data.len() {
                    let flags = u32::from_le_bytes([
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ]);
                    // Bit 1 (0x02) = kJournalInUse
                    (flags & 0x02) != 0
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub fn volume_name(&self) -> &str {
        self.header.volume_name()
    }

    fn fork_reader_from_extent(&self, extent: volume_header::HfsPlusExtentDescriptor) -> ForkReader<'_> {
        let fork_size = u64::from(extent.block_count) * u64::from(self.header.block_size);
        let mut reader = ForkReader::new(
            self.device.as_ref(),
            self.volume_offset,
            self.header.block_size,
            fork_size,
        );
        reader.set_extents(vec![extent]);
        reader
    }

    pub fn catalog_fork_reader(&self) -> ForkReader<'_> {
        self.fork_reader_from_extent(self.header.catalog_file.clone())
    }

    pub fn extents_fork_reader(&self) -> ForkReader<'_> {
        self.fork_reader_from_extent(self.header.extents_file.clone())
    }

    pub fn attributes_fork_reader(&self) -> ForkReader<'_> {
        self.fork_reader_from_extent(self.header.attributes_file.clone())
    }

    pub fn list_root(&self) -> anyhow::Result<Vec<DirEntry>> {
        self.list_directory(1)
    }

    pub fn list_directory(&self, parent_id: u32) -> anyhow::Result<Vec<DirEntry>> {
        let cat_fork = self.catalog_fork_reader();
        let btree = BTreeReader::open(&cat_fork)?;
        let reader = CatalogReader::open(btree);
        let entries = reader.list_directory(parent_id)?;
        Ok(entries
            .into_iter()
            .map(|(name, record)| {
                let (is_dir, size) = match &record {
                    catalog::CatalogRecordData::Folder(_) => (true, 0),
                    catalog::CatalogRecordData::File(f) => (false, f.data_fork.logical_size),
                    _ => (false, 0),
                };
                DirEntry {
                    name: normalize_hfs_name(&name),
                    is_directory: is_dir,
                    size,
                }
            })
            .collect())
    }

    /// Resolve a path like "/dir/subdir/file" and return the catalog record.
    pub fn resolve_path(&self, path: &str) -> anyhow::Result<catalog::CatalogRecordData> {
        let cat_fork = self.catalog_fork_reader();
        let btree = BTreeReader::open(&cat_fork)?;
        let reader = CatalogReader::open(btree);

        let clean = path.trim_start_matches('/').trim_end_matches('/');
        if clean.is_empty() {
            anyhow::bail!("Path must be an absolute path");
        }

        let mut parent_id = 1u32;
        let components: Vec<&str> = clean.split('/').collect();

        for (i, component) in components.iter().enumerate() {
            let is_last = i == components.len() - 1;
            let child = reader
                .find_child(parent_id, component)?
                .ok_or_else(|| anyhow::anyhow!("Path component '{}' not found", component))?;

            if is_last {
                return Ok(child);
            }

            match child {
                catalog::CatalogRecordData::Folder(f) => {
                    parent_id = f.folder_id;
                }
                _ => anyhow::bail!("'{}' is not a directory", component),
            }
        }

        anyhow::bail!("Empty path after resolution");
    }

    /// Build the full extent list for a fork, including overflow extents.
    fn build_extents(
        &self,
        fork_data: &HfsPlusForkData,
        file_id: u32,
        fork_type: u8,
    ) -> anyhow::Result<Vec<HfsPlusExtentDescriptor>> {
        let mut extents: Vec<HfsPlusExtentDescriptor> = fork_data
            .extents
            .iter()
            .filter(|e| e.block_count > 0)
            .cloned()
            .collect();

        if fork_data.needs_overflow() {
            let ext_fork = self.extents_fork_reader();
            let btree = BTreeReader::open(&ext_fork)?;
            let overflow = ExtentsOverflowReader::open(btree);
            let overflow_extents = overflow.lookup_all_extents(file_id, fork_type)?;
            extents.extend(overflow_extents);
        }

        Ok(extents)
    }

    /// Read a file's data fork content, given its catalog record.
    /// Automatically decompresses HFS+ compressed files.
    pub fn read_file_data(&self, file_record: &catalog::CatalogFile) -> anyhow::Result<Vec<u8>> {
        let extents = self.build_extents(&file_record.data_fork, file_record.file_id, 0)?;

        let mut reader = ForkReader::new(
            self.device.as_ref(),
            self.volume_offset,
            self.header.block_size,
            file_record.data_fork.logical_size,
        );
        reader.set_extents(extents);
        let data = reader.read_all()?;

        if file_record.is_compressed() && compression::is_hfs_compressed(&data) {
            log::info!("Decompressing 'cmpf' file (type={})", {
                let t = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
                t
            });
            compression::decompress_cmpf(&data)
        } else {
            Ok(data)
        }
    }

    /// Read content of a file identified by absolute HFS+ path.
    pub fn read_file(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let record = self.resolve_path(path)?;
        match record {
            catalog::CatalogRecordData::File(f) => self.read_file_data(&f),
            _ => anyhow::bail!("'{}' is not a file", path),
        }
    }

    pub fn extract_file(&self, src: &str, dst: &std::path::Path) -> anyhow::Result<u64> {
        let data = self.read_file(src)?;
        let len = data.len() as u64;
        std::fs::write(dst, &data)?;
        Ok(len)
    }
}
