use crate::hfs::btree::key::{HfsCatalogKeyRaw, HfsPlusCatalogKeyRaw};
use crate::hfs::btree::{BTreeReader, BTreeRecord};
use crate::hfs::volume_header::{HfsExtentDescriptor, HfsPlusForkData};
use crate::util::{read_u16_be, read_u32_be};

#[derive(Debug)]
pub enum CatalogRecordData {
    Folder(CatalogFolder),
    File(CatalogFile),
    Thread(CatalogThread),
}

#[derive(Debug)]
pub struct CatalogFolder {
    pub record_type: u16,
    pub flags: u16,
    pub valence: u32,
    pub folder_id: u32,
    pub create_date: u32,
    pub content_mod_date: u32,
    pub attribute_mod_date: u32,
    pub access_date: u32,
    pub backup_date: u32,
    pub text_encoding: u32,
    pub folder_count: u32,
}

#[derive(Debug)]
pub struct CatalogFile {
    pub record_type: u16,
    pub flags: u16,
    pub file_id: u32,
    pub create_date: u32,
    pub content_mod_date: u32,
    pub attribute_mod_date: u32,
    pub access_date: u32,
    pub backup_date: u32,
    pub text_encoding: u32,
    pub data_fork: HfsPlusForkData,
    pub resource_fork: HfsPlusForkData,
}

impl CatalogFile {
    /// Returns true if the file has the kHFSPlusFileHasExtentAttributeEncoded flag set,
    /// indicating the data fork uses HFS+ compression.
    pub fn is_compressed(&self) -> bool {
        self.flags & 0x0001 != 0
    }
}

#[derive(Debug)]
pub struct CatalogThread {
    pub record_type: u16,
    pub node_name: String,
}

#[derive(Debug)]
pub struct CatalogEntry {
    pub parent_id: u32,
    pub name: String,
    pub record: CatalogRecordData,
}

impl CatalogRecordData {
    pub fn file_id(&self) -> Option<u32> {
        match self {
            Self::File(f) => Some(f.file_id),
            Self::Folder(f) => Some(f.folder_id),
            _ => None,
        }
    }

    pub fn is_directory(&self) -> bool {
        matches!(self, Self::Folder(_))
    }
}

pub fn parse_catalog_record(value: &[u8]) -> anyhow::Result<CatalogRecordData> {
    if value.len() < 4 {
        anyhow::bail!("Catalog record too short");
    }

    let record_type = read_u16_be(value);

    match record_type {
        0x0001 => {
            if value.len() < 72 {
                anyhow::bail!("Folder record too short: {}", value.len());
            }
            Ok(CatalogRecordData::Folder(CatalogFolder {
                record_type,
                flags: read_u16_be(&value[2..]),
                valence: read_u32_be(&value[4..]),
                folder_id: read_u32_be(&value[8..]),
                create_date: read_u32_be(&value[12..]),
                content_mod_date: read_u32_be(&value[16..]),
                attribute_mod_date: read_u32_be(&value[20..]),
                access_date: read_u32_be(&value[24..]),
                backup_date: read_u32_be(&value[28..]),
                text_encoding: read_u32_be(&value[64..]),
                folder_count: read_u32_be(&value[68..]),
            }))
        }
        0x0002 => {
            // FileRecord: dataFork at offset 72, resourceFork at offset 152
            if value.len() < 232 {
                anyhow::bail!("File record too short: {}", value.len());
            }
            let data_fork = HfsPlusForkData::parse(&value[72..]);
            let resource_fork = HfsPlusForkData::parse(&value[152..]);

            Ok(CatalogRecordData::File(CatalogFile {
                record_type,
                flags: read_u16_be(&value[2..]),
                file_id: read_u32_be(&value[8..]),
                create_date: read_u32_be(&value[12..]),
                content_mod_date: read_u32_be(&value[16..]),
                attribute_mod_date: read_u32_be(&value[20..]),
                access_date: read_u32_be(&value[24..]),
                backup_date: read_u32_be(&value[28..]),
                text_encoding: read_u32_be(&value[64..]),
                data_fork,
                resource_fork,
            }))
        }
        0x0003 | 0x0004 => {
            if value.len() < 6 {
                anyhow::bail!("Thread record too short");
            }
            let name_len = read_u16_be(&value[4..]) as usize;
            let name = if name_len > 0 && value.len() >= 6 + name_len * 2 {
                crate::hfs::unicode::utf16be_to_string(&value[6..6 + name_len * 2])
            } else {
                String::new()
            };
            Ok(CatalogRecordData::Thread(CatalogThread {
                record_type,
                node_name: name,
            }))
        }
        _ => anyhow::bail!("Unknown catalog record type: {:#06x}", record_type),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder_record_bytes() -> Vec<u8> {
        let mut d = vec![0u8; 72];
        d[0..2].copy_from_slice(&1u16.to_be_bytes()); // recordType = kHFSPlusFolderRecord
        d[2..4].copy_from_slice(&0u16.to_be_bytes()); // flags
        d[4..8].copy_from_slice(&3u32.to_be_bytes()); // valence = 3 children
        d[8..12].copy_from_slice(&1u32.to_be_bytes()); // folderID = 1 (root)
        d[64..68].copy_from_slice(&2u32.to_be_bytes()); // textEncoding
        d[68..72].copy_from_slice(&1u32.to_be_bytes()); // folderCount
        d
    }

    fn file_record_bytes() -> Vec<u8> {
        let mut d = vec![0u8; 232];
        d[0..2].copy_from_slice(&2u16.to_be_bytes()); // recordType = kHFSPlusFileRecord
        d[2..4].copy_from_slice(&1u16.to_be_bytes()); // flags (compressed)
        d[8..12].copy_from_slice(&42u32.to_be_bytes()); // fileID
        d[64..68].copy_from_slice(&0u32.to_be_bytes()); // textEncoding
        // dataFork at offset 72
        d[72..80].copy_from_slice(&1024u64.to_be_bytes()); // logicalSize
        d[84..88].copy_from_slice(&2u32.to_be_bytes()); // totalBlocks
        d[88..96].copy_from_slice(&[0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x02]); // extent
        // resourceFork at offset 152
        d[152..160].copy_from_slice(&0u64.to_be_bytes()); // logicalSize = 0
        d
    }

    fn thread_record_bytes() -> Vec<u8> {
        let name = "somename";
        let name_utf16: Vec<u16> = name.encode_utf16().collect();
        let size = 6 + name_utf16.len() * 2;
        let mut d = vec![0u8; size];
        d[0..2].copy_from_slice(&3u16.to_be_bytes()); // recordType = kHFSPlusFolderThread
        d[4..6].copy_from_slice(&(name_utf16.len() as u16).to_be_bytes());
        for (i, &c) in name_utf16.iter().enumerate() {
            d[6 + i * 2..8 + i * 2].copy_from_slice(&c.to_be_bytes());
        }
        d
    }

    #[test]
    fn test_parse_folder_record() {
        let rec = parse_catalog_record(&folder_record_bytes()).unwrap();
        match rec {
            CatalogRecordData::Folder(f) => {
                assert_eq!(f.record_type, 1);
                assert_eq!(f.flags, 0);
                assert_eq!(f.valence, 3);
                assert_eq!(f.folder_id, 1);
                assert_eq!(f.text_encoding, 2);
                assert_eq!(f.folder_count, 1);
            }
            _ => panic!("Expected Folder"),
        }
    }

    #[test]
    fn test_parse_file_record() {
        let rec = parse_catalog_record(&file_record_bytes()).unwrap();
        match rec {
            CatalogRecordData::File(f) => {
                assert_eq!(f.record_type, 2);
                assert_eq!(f.flags, 1);
                assert_eq!(f.file_id, 42);
                assert!(f.is_compressed());
                assert_eq!(f.data_fork.logical_size, 1024);
                assert_eq!(f.data_fork.total_blocks, 2);
                assert_eq!(f.data_fork.extents[0].start_block, 10);
                assert_eq!(f.resource_fork.logical_size, 0);
            }
            _ => panic!("Expected File"),
        }
    }

    #[test]
    fn test_parse_thread_record_folder_thread() {
        let rec = parse_catalog_record(&thread_record_bytes()).unwrap();
        match rec {
            CatalogRecordData::Thread(t) => {
                assert_eq!(t.record_type, 0x0003);
                assert_eq!(t.node_name, "somename");
            }
            _ => panic!("Expected Thread"),
        }
    }

    #[test]
    fn test_parse_thread_record_file_thread() {
        let mut d = thread_record_bytes();
        d[0..2].copy_from_slice(&4u16.to_be_bytes());
        let rec = parse_catalog_record(&d).unwrap();
        match rec {
            CatalogRecordData::Thread(t) => {
                assert_eq!(t.record_type, 0x0004);
            }
            _ => panic!("Expected Thread"),
        }
    }

    #[test]
    fn test_parse_folder_record_too_short() {
        let d = vec![0u8; 10];
        assert!(parse_catalog_record(&d).is_err());
    }

    #[test]
    fn test_parse_file_record_too_short() {
        let d = vec![0u8; 100];
        assert!(parse_catalog_record(&d).is_err());
    }

    #[test]
    fn test_parse_thread_record_too_short() {
        let mut d = vec![0u8; 4];
        d[0..2].copy_from_slice(&3u16.to_be_bytes());
        assert!(parse_catalog_record(&d).is_err());
    }

    #[test]
    fn test_parse_unknown_record_type() {
        let mut d = vec![0u8; 10];
        d[0..2].copy_from_slice(&0xFFFFu16.to_be_bytes());
        assert!(parse_catalog_record(&d).is_err());
    }

    #[test]
    fn test_file_id() {
        let folder = CatalogRecordData::Folder(CatalogFolder {
            record_type: 1,
            flags: 0,
            valence: 1,
            folder_id: 5,
            create_date: 0,
            content_mod_date: 0,
            attribute_mod_date: 0,
            access_date: 0,
            backup_date: 0,
            text_encoding: 0,
            folder_count: 0,
        });
        assert_eq!(folder.file_id(), Some(5));
        assert!(folder.is_directory());

        let file = CatalogRecordData::File(CatalogFile {
            record_type: 2,
            flags: 0,
            file_id: 99,
            create_date: 0,
            content_mod_date: 0,
            attribute_mod_date: 0,
            access_date: 0,
            backup_date: 0,
            text_encoding: 0,
            data_fork: HfsPlusForkData::parse(&[0u8; 80]),
            resource_fork: HfsPlusForkData::parse(&[0u8; 80]),
        });
        assert_eq!(file.file_id(), Some(99));
        assert!(!file.is_directory());

        let thread = CatalogRecordData::Thread(CatalogThread {
            record_type: 3,
            node_name: String::new(),
        });
        assert_eq!(thread.file_id(), None);
        assert!(!thread.is_directory());
    }

    #[test]
    fn test_file_not_compressed() {
        let mut d = file_record_bytes();
        d[2..4].copy_from_slice(&0u16.to_be_bytes());
        let rec = parse_catalog_record(&d).unwrap();
        match rec {
            CatalogRecordData::File(f) => assert!(!f.is_compressed()),
            _ => panic!("Expected File"),
        }
    }
}

// ── HFS (original) catalog record types ──

#[derive(Debug)]
pub struct HfsCatalogFolder {
    pub record_type: u16,
    pub flags: u16,
    pub valence: u16,
    pub folder_id: u32,
    pub create_date: u32,
    pub modify_date: u32,
    pub backup_date: u32,
}

#[derive(Debug)]
pub struct HfsCatalogFile {
    pub record_type: u16,
    pub flags: u16,
    pub file_id: u32,
    pub data_start_block: u32, // flStBlk (in 512-byte blocks, i.e. allocation blocks)
    pub data_logical_size: u32, // flLogLen
    pub data_physical_size: u32, // flPyLen
    pub data_extents: Vec<HfsExtentDescriptor>,
    pub resource_start_block: u32,
    pub resource_logical_size: u32,
    pub resource_physical_size: u32,
    pub resource_extents: Vec<HfsExtentDescriptor>,
}

/// Parse an HFS (original) catalog record value.
/// All multi-byte fields are big-endian.
/// Ref: Apple hfs_format.h `HFSCatalogFolder` (62 bytes) and `HFSCatalogFile` (102 bytes).
pub fn parse_hfs_catalog_record(value: &[u8]) -> anyhow::Result<HfsCatalogRecord> {
    if value.len() < 4 {
        anyhow::bail!("HFS catalog record too short");
    }
    let record_type = read_u16_be(value);
    match record_type {
        0x0100 => {
            // HFSCatalogFolder: 62 bytes
            if value.len() < 62 {
                anyhow::bail!("HFS folder record too short: {}", value.len());
            }
            Ok(HfsCatalogRecord::Folder(HfsCatalogFolder {
                record_type,
                flags: read_u16_be(&value[2..]),
                valence: read_u16_be(&value[4..]),
                folder_id: read_u32_be(&value[6..]),
                create_date: read_u32_be(&value[10..]),
                modify_date: read_u32_be(&value[14..]),
                backup_date: read_u32_be(&value[18..]),
            }))
        }
        0x0200 => {
            // HFSCatalogFile: 102 bytes
            // Offsets per Apple hfs_format.h:
            //  +0 recordType (U16)
            //  +2 flags (U16)
            //  +4 userInfo (16 bytes, Finder file info)
            // +20 fileID (U32)
            // +24 dataStartBlock (U16, unused/0)
            // +26 dataLogicalSize (S32)
            // +30 dataPhysicalSize (S32)
            // +34 rsrcStartBlock (U16)
            // +36 rsrcLogicalSize (S32)
            // +40 rsrcPhysicalSize (S32)
            // +74 dataExtents (3 * [U16 start, U16 count] = 12 bytes)
            // +86 rsrcExtents (12 bytes)
            if value.len() < 102 {
                anyhow::bail!("HFS file record too short: {}", value.len());
            }
            Ok(HfsCatalogRecord::File(HfsCatalogFile {
                record_type,
                flags: read_u16_be(&value[2..]),
                file_id: read_u32_be(&value[20..]),
                data_start_block: read_u16_be(&value[24..]) as u32,
                data_logical_size: read_u32_be(&value[26..]),
                data_physical_size: read_u32_be(&value[30..]),
                resource_start_block: read_u16_be(&value[34..]) as u32,
                resource_logical_size: read_u32_be(&value[36..]),
                resource_physical_size: read_u32_be(&value[40..]),
                data_extents: parse_hfs_extent_record(&value[74..]),
                resource_extents: parse_hfs_extent_record(&value[86..]),
            }))
        }
        0x0300 | 0x0400 => Ok(HfsCatalogRecord::Thread),
        _ => anyhow::bail!("Unknown HFS catalog record type: {:#06x}", record_type),
    }
}

fn parse_hfs_extent_record(data: &[u8]) -> Vec<HfsExtentDescriptor> {
    let mut extents = Vec::new();
    for i in 0..3 {
        let off = i * 4;
        if off + 4 <= data.len() {
            let start = read_u16_be(&data[off..]);
            let count = read_u16_be(&data[off + 2..]);
            if count > 0 {
                extents.push(HfsExtentDescriptor {
                    start_block: start,
                    block_count: count,
                });
            }
        }
    }
    extents
}

#[derive(Debug)]
pub enum HfsCatalogRecord {
    Folder(HfsCatalogFolder),
    File(HfsCatalogFile),
    Thread,
}

pub struct HfsCatalogReader<'a> {
    tree: BTreeReader<'a>,
}

impl<'a> HfsCatalogReader<'a> {
    pub fn open(tree: BTreeReader<'a>) -> Self {
        Self { tree }
    }

    fn all_records(&self) -> anyhow::Result<Vec<BTreeRecord>> {
        let leaf_nodes = self.tree.iter_leaf_nodes()?;
        Ok(leaf_nodes.into_iter().flatten().collect())
    }

    pub fn list_directory(
        &self,
        parent_id: u32,
    ) -> anyhow::Result<Vec<(String, HfsCatalogRecord)>> {
        let all_records = self.all_records()?;
        let mut entries = Vec::new();
        for rec in &all_records {
            let key = HfsCatalogKeyRaw {
                data: rec.key.clone(),
            };
            if key.parent_id() != parent_id {
                continue;
            }
            match parse_hfs_catalog_record(&rec.value) {
                Ok(data) => {
                    let name = key.node_name();
                    if !name.is_empty() {
                        entries.push((name, data));
                    }
                }
                Err(e) => log::debug!("Skipping HFS record: {}", e),
            }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(entries)
    }

    pub fn find_child(
        &self,
        parent_id: u32,
        name: &str,
    ) -> anyhow::Result<Option<HfsCatalogRecord>> {
        let all_records = self.all_records()?;
        for rec in &all_records {
            let key = HfsCatalogKeyRaw {
                data: rec.key.clone(),
            };
            if key.parent_id() != parent_id {
                continue;
            }
            if key.node_name() == name
                && let Ok(data) = parse_hfs_catalog_record(&rec.value)
            {
                return Ok(Some(data));
            }
        }
        Ok(None)
    }
}

pub struct CatalogReader<'a> {
    tree: BTreeReader<'a>,
    case_sensitive: bool,
}

impl<'a> CatalogReader<'a> {
    pub fn open(tree: BTreeReader<'a>) -> Self {
        let case_sensitive = !tree.is_case_insensitive();
        Self {
            tree,
            case_sensitive,
        }
    }

    pub fn with_case_sensitivity(tree: BTreeReader<'a>, case_sensitive: bool) -> Self {
        Self {
            tree,
            case_sensitive,
        }
    }

    fn all_records(&self) -> anyhow::Result<Vec<BTreeRecord>> {
        let leaf_nodes = self.tree.iter_leaf_nodes()?;
        Ok(leaf_nodes.into_iter().flatten().collect())
    }

    pub fn list_root(&self) -> anyhow::Result<Vec<(String, CatalogRecordData)>> {
        self.list_directory(1)
    }

    pub fn list_directory(
        &self,
        parent_id: u32,
    ) -> anyhow::Result<Vec<(String, CatalogRecordData)>> {
        let all_records = self.all_records()?;
        let mut entries = Vec::new();

        for rec in &all_records {
            let key = HfsPlusCatalogKeyRaw {
                data: rec.key.clone(),
            };
            if key.parent_id() != parent_id {
                continue;
            }
            match parse_catalog_record(&rec.value) {
                Ok(data) => {
                    let name = key.node_name();
                    if !name.is_empty() {
                        entries.push((name, data));
                    }
                }
                Err(e) => log::debug!("Skipping record: {}", e),
            }
        }

        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(entries)
    }

    pub fn find_child(
        &self,
        parent_id: u32,
        name: &str,
    ) -> anyhow::Result<Option<CatalogRecordData>> {
        let all_records = self.all_records()?;
        let folded_name = if !self.case_sensitive {
            Some(crate::hfs::unicode::case_fold(name))
        } else {
            None
        };

        for rec in &all_records {
            let key = HfsPlusCatalogKeyRaw {
                data: rec.key.clone(),
            };
            if key.parent_id() != parent_id {
                continue;
            }
            let node_name = key.node_name();
            let matched = if self.case_sensitive {
                node_name == name
            } else {
                folded_name
                    .as_ref()
                    .is_some_and(|f| crate::hfs::unicode::case_fold(&node_name) == *f)
            };
            if !matched {
                continue;
            }
            if let Ok(data) = parse_catalog_record(&rec.value) {
                return Ok(Some(data));
            }
        }
        Ok(None)
    }
}
