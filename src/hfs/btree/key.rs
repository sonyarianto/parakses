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
