use crate::util::{read_u16_be, read_u32_be};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    HeaderNode = 0,
    IndexNode = 1,
    LeafNode = 2,
    MapNode = 3,
}

impl NodeType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::HeaderNode),
            1 => Some(Self::IndexNode),
            2 => Some(Self::LeafNode),
            3 => Some(Self::MapNode),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct NodeDescriptor {
    pub f_link: u32,
    pub b_link: u32,
    pub kind: NodeType,
    pub height: u8,
    pub num_records: u16,
}

impl NodeDescriptor {
    pub fn parse(data: &[u8]) -> anyhow::Result<Self> {
        if data.len() < 14 {
            anyhow::bail!("Node descriptor too short");
        }
        let f_link = read_u32_be(data);
        let b_link = read_u32_be(&data[4..]);
        let kind = NodeType::from_u8(data[8])
            .ok_or_else(|| anyhow::anyhow!("Invalid node type: {}", data[8]))?;
        let height = data[9];
        let num_records = read_u16_be(&data[10..]);

        Ok(Self {
            f_link,
            b_link,
            kind,
            height,
            num_records,
        })
    }
}

#[derive(Debug)]
pub struct HeaderRecord {
    pub tree_depth: u16,
    pub root_node: u32,
    pub leaf_records: u32,
    pub first_leaf_node: u32,
    pub last_leaf_node: u32,
    pub node_size: u16,
    pub max_key_len: u16,
    pub total_nodes: u32,
    pub free_nodes: u32,
    pub clump_size: u32,
    pub btree_type: u8,
    pub key_compare_type: u8,
    pub attributes: u32,
}

impl HeaderRecord {
    pub fn parse(data: &[u8]) -> anyhow::Result<Self> {
        if data.len() < 106 {
            anyhow::bail!("Header record data too short: {} bytes", data.len());
        }

        let node_size = read_u16_be(&data[32..]);
        if node_size < 512 {
            anyhow::bail!("B-tree node size too small: {} (minimum 512)", node_size);
        }
        if !node_size.is_power_of_two() {
            anyhow::bail!(
                "B-tree node size not power of two: {}",
                node_size
            );
        }

        Ok(Self {
            tree_depth: read_u16_be(&data[14..]),
            root_node: read_u32_be(&data[16..]),
            leaf_records: read_u32_be(&data[20..]),
            first_leaf_node: read_u32_be(&data[24..]),
            last_leaf_node: read_u32_be(&data[28..]),
            node_size,
            max_key_len: read_u16_be(&data[34..]),
            total_nodes: read_u32_be(&data[36..]),
            free_nodes: read_u32_be(&data[40..]),
            clump_size: read_u32_be(&data[44..]),
            btree_type: data[48],
            key_compare_type: data[49],
            attributes: read_u32_be(&data[50..]),
        })
    }

    /// Returns true if the btree uses case-insensitive (case-folding) key comparison.
    /// Key compare type: 0xBC = binary (case-sensitive), 0xCF = case-folding (case-insensitive).
    /// For HFS+, the value is typically 0xCF (kHFSCaseFolding).
    pub fn is_case_insensitive(&self) -> bool {
        self.key_compare_type == 0xCF
    }
}

pub fn record_offsets(node_data: &[u8], num_records: u16, node_size: u16) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(num_records as usize);
    let off_table_start = node_size as usize - (num_records as usize * 2);
    for i in 0..num_records as usize {
        let off = off_table_start + i * 2;
        if off + 2 <= node_data.len() {
            offsets.push(read_u16_be(&node_data[off..]) as usize);
        }
    }
    offsets
}

pub fn read_record<'a>(node_data: &'a [u8], offset: usize, next_offset: usize) -> &'a [u8] {
    let end = next_offset.min(node_data.len());
    &node_data[offset..end]
}
