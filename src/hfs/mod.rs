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
use volume_header::{
    HfsExtentDescriptor, HfsMdb, HfsPlusExtentDescriptor, HfsPlusForkData, VolumeHeader,
};

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
    pub is_hfs_original: bool,
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

enum VolumeKind {
    HfsPlus { header: VolumeHeader },
    HfsOriginal { mdb: HfsMdb },
}

pub struct HfsVolume {
    device: Box<dyn BlockDevice>,
    volume_offset: u64,
    kind: VolumeKind,
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

        // Try HFS+ first, fall back to HFS original
        if let Ok(header) = VolumeHeader::parse(header_data) {
            return Ok(Self {
                device,
                volume_offset,
                kind: VolumeKind::HfsPlus { header },
            });
        }

        let mdb = HfsMdb::parse(header_data)?;
        Ok(Self {
            device,
            volume_offset,
            kind: VolumeKind::HfsOriginal { mdb },
        })
    }

    fn block_size(&self) -> u32 {
        match &self.kind {
            VolumeKind::HfsPlus { header } => header.block_size,
            VolumeKind::HfsOriginal { mdb } => mdb.alloc_block_size,
        }
    }

    fn total_blocks(&self) -> u32 {
        match &self.kind {
            VolumeKind::HfsPlus { header } => header.total_blocks,
            VolumeKind::HfsOriginal { mdb } => mdb.num_alloc_blocks as u32,
        }
    }

    fn free_blocks(&self) -> u32 {
        match &self.kind {
            VolumeKind::HfsPlus { header } => header.free_blocks,
            VolumeKind::HfsOriginal { mdb } => mdb.free_blocks as u32,
        }
    }

    fn file_count(&self) -> u32 {
        match &self.kind {
            VolumeKind::HfsPlus { header } => header.file_count,
            VolumeKind::HfsOriginal { mdb } => mdb.file_count,
        }
    }

    fn folder_count(&self) -> u32 {
        match &self.kind {
            VolumeKind::HfsPlus { header } => header.folder_count,
            VolumeKind::HfsOriginal { mdb } => mdb.folder_count,
        }
    }

    fn write_count(&self) -> u32 {
        match &self.kind {
            VolumeKind::HfsPlus { header } => header.write_count,
            VolumeKind::HfsOriginal { mdb } => mdb.write_count,
        }
    }

    pub fn header(&self) -> Option<&VolumeHeader> {
        match &self.kind {
            VolumeKind::HfsPlus { header } => Some(header),
            VolumeKind::HfsOriginal { .. } => None,
        }
    }

    pub fn mdb(&self) -> Option<&HfsMdb> {
        match &self.kind {
            VolumeKind::HfsPlus { .. } => None,
            VolumeKind::HfsOriginal { mdb } => Some(mdb),
        }
    }

    pub fn volume_info(&self) -> VolumeInfo {
        let (signature, version, volume_name, is_hfsx, is_journaled, journal_dirty) =
            match &self.kind {
                VolumeKind::HfsPlus { header } => {
                    let (is_j, j_dirty) = if header.is_journaled() {
                        (true, self.check_journal_dirty())
                    } else {
                        (false, false)
                    };
                    (
                        header.signature,
                        header.version,
                        header.volume_name().to_string(),
                        header.is_hfsx(),
                        is_j,
                        j_dirty,
                    )
                }
                VolumeKind::HfsOriginal { mdb } => (
                    mdb.signature,
                    0,
                    mdb.volume_name.clone(),
                    false,
                    false,
                    false,
                ),
            };

        let is_hfs_original = matches!(&self.kind, VolumeKind::HfsOriginal { .. });
        VolumeInfo {
            signature,
            version,
            volume_name,
            block_size: self.block_size(),
            total_blocks: self.total_blocks(),
            free_blocks: self.free_blocks(),
            file_count: self.file_count(),
            folder_count: self.folder_count(),
            is_hfsx,
            is_hfs_original,
            is_journaled,
            journal_dirty,
            write_count: self.write_count(),
        }
    }

    fn check_journal_dirty(&self) -> bool {
        let journal_info_block = match &self.kind {
            VolumeKind::HfsPlus { header } => header.journal_info_block,
            VolumeKind::HfsOriginal { .. } => return false,
        };
        let journal_lba =
            u64::from(journal_info_block) * u64::from(self.block_size()) + self.volume_offset;
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
                    (flags & 0x02) != 0
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub fn volume_name(&self) -> String {
        match &self.kind {
            VolumeKind::HfsPlus { header } => header.volume_name().to_string(),
            VolumeKind::HfsOriginal { mdb } => mdb.volume_name.clone(),
        }
    }

    fn build_fork_from_extent(&self, start_block: u64, block_count: u64) -> ForkReader<'_> {
        let fork_size = block_count * u64::from(self.block_size());
        let mut reader = ForkReader::new(
            self.device.as_ref(),
            self.volume_offset,
            self.block_size(),
            fork_size,
        );
        reader.set_extents(vec![HfsPlusExtentDescriptor {
            start_block: start_block as u32,
            block_count: block_count as u32,
        }]);
        reader
    }

    fn first_alloc_block(&self) -> u32 {
        match &self.kind {
            VolumeKind::HfsPlus { .. } => 0,
            VolumeKind::HfsOriginal { mdb } => mdb.first_alloc_block as u32,
        }
    }

    fn build_fork_from_hfs_extents(
        &self,
        extents: &[HfsExtentDescriptor],
        logical_size: u64,
    ) -> ForkReader<'_> {
        let base = self.first_alloc_block();
        let converted: Vec<HfsPlusExtentDescriptor> = extents
            .iter()
            .filter(|e| e.block_count > 0)
            .map(|e| HfsPlusExtentDescriptor {
                // HFS extent records store allocation block numbers;
                // convert to physical block numbers by adding drAlBlSt.
                start_block: base + e.start_block as u32,
                block_count: e.block_count as u32,
            })
            .collect();
        let mut reader = ForkReader::new(
            self.device.as_ref(),
            self.volume_offset,
            self.block_size(),
            logical_size,
        );
        reader.set_extents(converted);
        reader
    }

    pub fn catalog_fork_reader(&self) -> ForkReader<'_> {
        match &self.kind {
            VolumeKind::HfsPlus { header } => {
                let extent = header.catalog_file.clone();
                self.build_fork_from_extent(extent.start_block as u64, extent.block_count as u64)
            }
            VolumeKind::HfsOriginal { mdb } => {
                let extents = &mdb.ct_extents;
                self.build_fork_from_hfs_extents(extents, mdb.ct_fl_size as u64)
            }
        }
    }

    pub fn extents_fork_reader(&self) -> ForkReader<'_> {
        match &self.kind {
            VolumeKind::HfsPlus { header } => {
                let extent = header.extents_file.clone();
                self.build_fork_from_extent(extent.start_block as u64, extent.block_count as u64)
            }
            VolumeKind::HfsOriginal { mdb } => {
                let extents = &mdb.xt_extents;
                self.build_fork_from_hfs_extents(extents, mdb.xt_fl_size as u64)
            }
        }
    }

    pub fn attributes_fork_reader(&self) -> ForkReader<'_> {
        match &self.kind {
            VolumeKind::HfsPlus { header } => {
                let extent = header.attributes_file.clone();
                self.build_fork_from_extent(extent.start_block as u64, extent.block_count as u64)
            }
            VolumeKind::HfsOriginal { .. } => {
                // HFS original has no attributes file
                ForkReader::new(self.device.as_ref(), self.volume_offset, 512, 0)
            }
        }
    }

    pub fn list_root(&self) -> anyhow::Result<Vec<DirEntry>> {
        let root_id = match &self.kind {
            VolumeKind::HfsPlus { .. } => 1,
            VolumeKind::HfsOriginal { .. } => 2,
        };
        self.list_directory(root_id)
    }

    pub fn list_directory(&self, parent_id: u32) -> anyhow::Result<Vec<DirEntry>> {
        match &self.kind {
            VolumeKind::HfsPlus { .. } => {
                let cat_fork = self.catalog_fork_reader();
                let btree = BTreeReader::open(&cat_fork)?;
                let reader = CatalogReader::open(btree);
                let entries = reader.list_directory(parent_id)?;
                Ok(entries
                    .into_iter()
                    .map(|(name, record)| {
                        let (is_dir, size) = match &record {
                            catalog::CatalogRecordData::Folder(_) => (true, 0),
                            catalog::CatalogRecordData::File(f) => {
                                (false, f.data_fork.logical_size)
                            }
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
            VolumeKind::HfsOriginal { .. } => {
                let cat_fork = self.catalog_fork_reader();
                let btree = BTreeReader::open(&cat_fork)?;
                let reader = catalog::HfsCatalogReader::open(btree);
                let entries = reader.list_directory(parent_id)?;
                Ok(entries
                    .into_iter()
                    .map(|(name, record)| {
                        let (is_dir, size) = match &record {
                            catalog::HfsCatalogRecord::Folder(_) => (true, 0),
                            catalog::HfsCatalogRecord::File(f) => {
                                (false, f.data_logical_size as u64)
                            }
                            _ => (false, 0),
                        };
                        DirEntry {
                            name,
                            is_directory: is_dir,
                            size,
                        }
                    })
                    .collect())
            }
        }
    }

    pub fn resolve_path(&self, path: &str) -> anyhow::Result<catalog::CatalogRecordData> {
        match &self.kind {
            VolumeKind::HfsPlus { .. } => self.resolve_path_hfs_plus(path),
            VolumeKind::HfsOriginal { .. } => {
                let record = self.resolve_path_hfs_original(path)?;
                // Convert to CatalogRecordData for API compatibility
                match record {
                    catalog::HfsCatalogRecord::Folder(f) => {
                        Ok(catalog::CatalogRecordData::Folder(catalog::CatalogFolder {
                            record_type: f.record_type,
                            flags: f.flags,
                            valence: f.valence as u32,
                            folder_id: f.folder_id,
                            create_date: f.create_date,
                            content_mod_date: f.modify_date,
                            attribute_mod_date: 0,
                            access_date: 0,
                            backup_date: f.backup_date,
                            text_encoding: 0,
                            folder_count: 0,
                        }))
                    }
                    catalog::HfsCatalogRecord::File(f) => {
                        let base = self.first_alloc_block();
                        let mut df_data = vec![0u8; 80];
                        df_data[0..8].copy_from_slice(&(f.data_logical_size as u64).to_be_bytes());
                        df_data[12..16].copy_from_slice(&1u32.to_be_bytes());
                        // Build extent record from HFS extents
                        let mut ext_buf = Vec::new();
                        for ext in &f.data_extents {
                            if ext.block_count == 0 {
                                continue;
                            }
                            let start = base + ext.start_block as u32;
                            ext_buf.extend_from_slice(&start.to_be_bytes());
                            ext_buf.extend_from_slice(&(ext.block_count as u32).to_be_bytes());
                        }
                        // Pad to 8 extents (64 bytes)
                        ext_buf.resize(64, 0u8);
                        df_data[16..80].copy_from_slice(&ext_buf);
                        let data_fork = HfsPlusForkData::parse(&df_data);

                        let rf_data = vec![0u8; 80];
                        let resource_fork = HfsPlusForkData::parse(&rf_data);

                        Ok(catalog::CatalogRecordData::File(catalog::CatalogFile {
                            record_type: f.record_type,
                            flags: f.flags,
                            file_id: f.file_id,
                            create_date: 0,
                            content_mod_date: 0,
                            attribute_mod_date: 0,
                            access_date: 0,
                            backup_date: 0,
                            text_encoding: 0,
                            data_fork,
                            resource_fork,
                        }))
                    }
                    _ => anyhow::bail!("Path resolved to a thread record"),
                }
            }
        }
    }

    fn resolve_path_hfs_plus(&self, path: &str) -> anyhow::Result<catalog::CatalogRecordData> {
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

    fn resolve_path_hfs_original(&self, path: &str) -> anyhow::Result<catalog::HfsCatalogRecord> {
        let cat_fork = self.catalog_fork_reader();
        let btree = BTreeReader::open(&cat_fork)?;
        let reader = catalog::HfsCatalogReader::open(btree);

        let clean = path.trim_start_matches('/').trim_end_matches('/');
        if clean.is_empty() {
            anyhow::bail!("Path must be an absolute path");
        }

        // HFS original root folder ID is 2 (kHFSRootFolderID)
        let mut parent_id = 2u32;
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
                catalog::HfsCatalogRecord::Folder(f) => {
                    parent_id = f.folder_id;
                }
                _ => anyhow::bail!("'{}' is not a directory", component),
            }
        }

        anyhow::bail!("Empty path after resolution");
    }

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

    pub fn read_file_data(&self, file_record: &catalog::CatalogFile) -> anyhow::Result<Vec<u8>> {
        let extents = self.build_extents(&file_record.data_fork, file_record.file_id, 0)?;

        let block_size = self.block_size();
        let mut reader = ForkReader::new(
            self.device.as_ref(),
            self.volume_offset,
            block_size,
            file_record.data_fork.logical_size,
        );
        reader.set_extents(extents);
        let data = reader.read_all()?;

        if file_record.is_compressed() && compression::is_hfs_compressed(&data) {
            log::info!(
                "Decompressing 'cmpf' file (type={})",
                u32::from_le_bytes([data[4], data[5], data[6], data[7]])
            );
            compression::decompress_cmpf(&data)
        } else {
            Ok(data)
        }
    }

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
