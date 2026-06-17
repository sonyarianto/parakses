use crate::hfs::btree::key::HfsPlusCatalogKeyRaw;
use crate::hfs::btree::{BTreeReader, BTreeRecord};
use crate::hfs::volume_header::HfsPlusForkData;
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

pub struct CatalogReader<'a> {
    tree: BTreeReader<'a>,
    case_sensitive: bool,
}

impl<'a> CatalogReader<'a> {
    pub fn open(tree: BTreeReader<'a>) -> Self {
        let case_sensitive = !tree.is_case_insensitive();
        Self { tree, case_sensitive }
    }

    pub fn with_case_sensitivity(tree: BTreeReader<'a>, case_sensitive: bool) -> Self {
        Self { tree, case_sensitive }
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
                folded_name.as_ref().map_or(false, |f| {
                    crate::hfs::unicode::case_fold(&node_name) == *f
                })
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
