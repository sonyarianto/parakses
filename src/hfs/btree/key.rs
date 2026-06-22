use crate::hfs::unicode;
use crate::util::{read_u16_be, read_u32_be};

pub trait BTreeKey: Sized {
    fn compare(&self, other: &Self) -> std::cmp::Ordering;
    fn parse(data: &[u8]) -> anyhow::Result<Self>;
    fn encoded_len(&self) -> u16;
}

#[derive(Debug)]
pub struct HfsPlusCatalogKeyRaw {
    pub data: Vec<u8>,
}

impl HfsPlusCatalogKeyRaw {
    pub fn parent_id(&self) -> u32 {
        if self.data.len() >= 6 {
            read_u32_be(&self.data)
        } else {
            0
        }
    }

    pub fn node_name(&self) -> String {
        if self.data.len() < 6 {
            return String::new();
        }
        let name_len = read_u16_be(&self.data[4..]) as usize;
        if self.data.len() < 6 + name_len * 2 {
            return String::new();
        }
        unicode::utf16be_to_string(&self.data[6..6 + name_len * 2])
    }

    pub fn raw_key_bytes(&self) -> &[u8] {
        &self.data
    }
}

#[derive(Debug)]
pub struct HfsPlusCatalogKey {
    pub parent_id: u32,
    pub node_name: String,
}

impl HfsPlusCatalogKey {
    pub fn from_raw(raw: &HfsPlusCatalogKeyRaw) -> Self {
        Self {
            parent_id: raw.parent_id(),
            node_name: raw.node_name(),
        }
    }

    pub fn raw_encode(&self) -> Vec<u8> {
        let name_utf16: Vec<u16> = self.node_name.encode_utf16().collect();
        let name_len = name_utf16.len() as u16;
        let total_len = 6 + name_len * 2;
        let mut buf = Vec::with_capacity(2 + total_len as usize);

        buf.extend_from_slice(&(total_len + 2).to_be_bytes());
        buf.extend_from_slice(&self.parent_id.to_be_bytes());
        buf.extend_from_slice(&name_len.to_be_bytes());
        for &c in &name_utf16 {
            buf.extend_from_slice(&c.to_be_bytes());
        }
        buf
    }
}

#[derive(Debug)]
pub struct HfsPlusExtentKey {
    pub file_id: u32,
    pub fork_type: u8,
    pub start_block: u32,
}

impl HfsPlusExtentKey {
    pub fn parse(raw: &[u8]) -> anyhow::Result<Self> {
        if raw.len() < 10 {
            anyhow::bail!("Extent key too short");
        }
        Ok(Self {
            file_id: read_u32_be(&raw[2..]),
            fork_type: raw[6],
            start_block: read_u32_be(&raw[7..]),
        })
    }
}

/// HFS (original) catalog key — uses Pascal string node names (MacRoman).
#[derive(Debug)]
pub struct HfsCatalogKeyRaw {
    pub data: Vec<u8>,
}

impl HfsCatalogKeyRaw {
    pub fn parent_id(&self) -> u32 {
        if self.data.len() >= 4 {
            read_u32_be(&self.data)
        } else {
            0
        }
    }

    pub fn node_name(&self) -> String {
        if self.data.len() < 5 {
            return String::new();
        }
        let name_len = self.data[4] as usize;
        let max_avail = self.data.len() - 5;
        let len = name_len.min(max_avail);
        // HFS original uses MacRoman encoding. For simplicity, treat as Latin-1.
        self.data[5..5 + len].iter().map(|&b| b as char).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_key_root_folder() {
        // Root folder: parentID=1, nameLength=0, name=""
        let key = HfsPlusCatalogKeyRaw {
            data: vec![0x00, 0x00, 0x00, 0x01, 0x00, 0x00],
        };
        assert_eq!(key.parent_id(), 1);
        assert_eq!(key.node_name(), "");
    }

    #[test]
    fn test_catalog_key_with_name() {
        // parentID=1, nameLength=9 ("hello.txt" in UTF-16BE)
        let mut data = vec![0x00, 0x00, 0x00, 0x01, 0x00, 0x09];
        let name_utf16: Vec<u16> = "hello.txt".encode_utf16().collect();
        for &c in &name_utf16 {
            data.extend_from_slice(&c.to_be_bytes());
        }
        let key = HfsPlusCatalogKeyRaw { data };
        assert_eq!(key.parent_id(), 1);
        assert_eq!(key.node_name(), "hello.txt");
    }

    #[test]
    fn test_catalog_key_too_short() {
        let key = HfsPlusCatalogKeyRaw {
            data: vec![0x00, 0x01],
        };
        assert_eq!(key.parent_id(), 0);
        assert_eq!(key.node_name(), "");
    }

    #[test]
    fn test_catalog_key_partial_name() {
        // parentID=2, nameLength=5 but data truncated
        let key = HfsPlusCatalogKeyRaw {
            data: vec![0x00, 0x00, 0x00, 0x02, 0x00, 0x05, 0x00],
        };
        assert_eq!(key.parent_id(), 2);
        assert_eq!(key.node_name(), "");
    }

    #[test]
    fn test_catalog_key_unicode_name() {
        let name = "caf\u{00E9}";
        let name_utf16: Vec<u16> = name.encode_utf16().collect();
        let mut data = vec![
            0x00,
            0x00,
            0x00,
            0x01,
            (name_utf16.len() as u16).to_be_bytes()[0],
            (name_utf16.len() as u16).to_be_bytes()[1],
        ];
        for &c in &name_utf16 {
            data.extend_from_slice(&c.to_be_bytes());
        }
        let key = HfsPlusCatalogKeyRaw { data };
        assert_eq!(key.parent_id(), 1);
        assert_eq!(key.node_name(), name);
    }

    #[test]
    fn test_catalog_key_raw_roundtrip() {
        let name = "SomeFile.txt";
        let key = HfsPlusCatalogKey {
            parent_id: 42,
            node_name: name.to_string(),
        };
        let encoded = key.raw_encode();
        assert!(encoded.len() > 8);

        // raw_encode produces: [keyLength:2][parentID:4][nameLength:2][name:...]
        let key_len = u16::from_be_bytes([encoded[0], encoded[1]]) as usize;
        assert_eq!(key_len, encoded.len());

        // Strip keyLength to get the raw key data
        let raw_key_data = &encoded[2..key_len];
        let parsed = HfsPlusCatalogKeyRaw {
            data: raw_key_data.to_vec(),
        };
        assert_eq!(parsed.parent_id(), 42);
        assert_eq!(parsed.node_name(), name);
    }

    #[test]
    fn test_extent_key_parse() {
        // On-disk format: keyLength(2) + fileID(4) + forkType(1) + startBlock(4)
        // data fork type = 0, fileID=2, startBlock=3
        let raw = [
            0x00, 0x09, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x03,
        ];
        let key = HfsPlusExtentKey::parse(&raw).unwrap();
        assert_eq!(key.file_id, 2);
        assert_eq!(key.fork_type, 0);
        assert_eq!(key.start_block, 3);
    }

    #[test]
    fn test_extent_key_data_fork() {
        let raw = b"\x00\x09\x00\x00\x00\x0A\x00\x00\x00\x00\x00\x00";
        let key = HfsPlusExtentKey::parse(raw).unwrap();
        assert_eq!(key.file_id, 10);
        assert_eq!(key.fork_type, 0);
        assert_eq!(key.start_block, 0);
    }

    #[test]
    fn test_extent_key_resource_fork() {
        let raw = b"\x00\x09\x00\x00\x00\x05\x01\x00\x00\x00\x00";
        let key = HfsPlusExtentKey::parse(raw).unwrap();
        assert_eq!(key.file_id, 5);
        assert_eq!(key.fork_type, 1);
        assert_eq!(key.start_block, 0);
    }

    #[test]
    fn test_extent_key_too_short() {
        assert!(HfsPlusExtentKey::parse(b"\x00\x05").is_err());
    }

    #[test]
    fn test_raw_key_bytes() {
        let data = vec![0x00, 0x00, 0x00, 0x01, 0x00, 0x00];
        let key = HfsPlusCatalogKeyRaw { data: data.clone() };
        assert_eq!(key.raw_key_bytes(), &data);
    }

    #[test]
    fn test_catalog_key_from_raw() {
        let raw = HfsPlusCatalogKeyRaw {
            data: vec![
                0x00, 0x00, 0x00, 0x07, 0x00, 0x03, 0x00, 0x61, 0x00, 0x62, 0x00, 0x63,
            ],
        };
        let parsed = HfsPlusCatalogKey::from_raw(&raw);
        assert_eq!(parsed.parent_id, 7);
        assert_eq!(parsed.node_name, "abc");
    }
}
