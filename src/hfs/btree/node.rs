use crate::util::{read_u16_be, read_u32_be};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    HeaderNode = 0,
    IndexNode = 1,
    LeafNode = 2,
    MapNode = 3,
}

impl NodeType {
    /// Standard HFS+ mapping:
    ///   0 = HeaderNode, 1 = IndexNode, 2 = LeafNode, 3 = MapNode
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::HeaderNode),
            1 => Some(Self::IndexNode),
            2 => Some(Self::LeafNode),
            3 => Some(Self::MapNode),
            _ => None,
        }
    }

    /// HFS (original) mapping:
    ///   0xFF = LeafNode, 0x00 = IndexNode, 0x01 = HeaderNode, 0x02 = MapNode
    pub fn from_hfs_u8(v: u8) -> Option<Self> {
        match v {
            0xFF => Some(Self::LeafNode),
            0x00 => Some(Self::IndexNode),
            0x01 => Some(Self::HeaderNode),
            0x02 => Some(Self::MapNode),
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
        Self::parse_with_mapping(data, false)
    }

    pub fn parse_hfs(data: &[u8]) -> anyhow::Result<Self> {
        Self::parse_with_mapping(data, true)
    }

    fn parse_with_mapping(data: &[u8], is_hfs_original: bool) -> anyhow::Result<Self> {
        if data.len() < 14 {
            anyhow::bail!("Node descriptor too short");
        }
        let f_link = read_u32_be(data);
        let b_link = read_u32_be(&data[4..]);
        let kind = if is_hfs_original {
            NodeType::from_hfs_u8(data[8])
        } else {
            NodeType::from_u8(data[8])
        }
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
            anyhow::bail!("B-tree node size not power of two: {}", node_size);
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
    // Offset table entries are stored from the end of the node downward.
    // The entry at nodeSize-2 points to record 0 (first record),
    // nodeSize-4 points to record 1, and so on.
    let mut offsets = Vec::with_capacity(num_records as usize);
    for i in 0..num_records as usize {
        let entry_off = (node_size as usize) - 2 - i * 2;
        if entry_off + 2 <= node_data.len() {
            offsets.push(read_u16_be(&node_data[entry_off..]) as usize);
        }
    }
    offsets
}

pub fn read_record(node_data: &[u8], offset: usize, next_offset: usize) -> &[u8] {
    let end = next_offset.min(node_data.len());
    &node_data[offset..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_node_descriptor() {
        let mut data = vec![0u8; 14];
        data[0..4].copy_from_slice(&0u32.to_be_bytes()); // fLink
        data[4..8].copy_from_slice(&0u32.to_be_bytes()); // bLink
        data[8] = 0; // kind = header
        data[9] = 0; // height
        data[10..12].copy_from_slice(&1u16.to_be_bytes()); // numRecords
        let desc = NodeDescriptor::parse(&data).unwrap();
        assert_eq!(desc.f_link, 0);
        assert_eq!(desc.b_link, 0);
        assert_eq!(desc.kind, NodeType::HeaderNode);
        assert_eq!(desc.height, 0);
        assert_eq!(desc.num_records, 1);
    }

    #[test]
    fn test_leaf_node_descriptor() {
        let mut data = vec![0u8; 14];
        data[0..4].copy_from_slice(&5u32.to_be_bytes()); // fLink
        data[4..8].copy_from_slice(&3u32.to_be_bytes()); // bLink
        data[8] = 2; // kind = leaf
        data[9] = 1; // height
        data[10..12].copy_from_slice(&2u16.to_be_bytes()); // numRecords
        let desc = NodeDescriptor::parse(&data).unwrap();
        assert_eq!(desc.f_link, 5);
        assert_eq!(desc.b_link, 3);
        assert_eq!(desc.kind, NodeType::LeafNode);
        assert_eq!(desc.height, 1);
        assert_eq!(desc.num_records, 2);
    }

    #[test]
    fn test_index_node_descriptor() {
        let mut data = vec![0u8; 14];
        data[8] = 1;
        let desc = NodeDescriptor::parse(&data).unwrap();
        assert_eq!(desc.kind, NodeType::IndexNode);
    }

    #[test]
    fn test_map_node_descriptor() {
        let mut data = vec![0u8; 14];
        data[8] = 3;
        let desc = NodeDescriptor::parse(&data).unwrap();
        assert_eq!(desc.kind, NodeType::MapNode);
    }

    #[test]
    fn test_reject_invalid_node_type() {
        let data = vec![0u8; 14];
        for &t in &[4u8, 5, 0xFF] {
            let mut d = data.clone();
            d[8] = t;
            assert!(NodeDescriptor::parse(&d).is_err());
        }
    }

    #[test]
    fn test_node_descriptor_too_short() {
        assert!(NodeDescriptor::parse(&[0u8; 13]).is_err());
        assert!(NodeDescriptor::parse(&[0u8; 0]).is_err());
    }

    #[test]
    fn test_header_record_parse() {
        let mut data = vec![0u8; 106];
        data[14..16].copy_from_slice(&1u16.to_be_bytes()); // treeDepth
        data[16..20].copy_from_slice(&6u32.to_be_bytes()); // rootNode
        data[20..24].copy_from_slice(&2u32.to_be_bytes()); // leafRecords
        data[24..28].copy_from_slice(&6u32.to_be_bytes()); // firstLeafNode
        data[28..32].copy_from_slice(&6u32.to_be_bytes()); // lastLeafNode
        data[32..34].copy_from_slice(&512u16.to_be_bytes()); // nodeSize
        data[34..36].copy_from_slice(&516u16.to_be_bytes()); // maxKeyLen
        data[36..40].copy_from_slice(&2u32.to_be_bytes()); // totalNodes
        data[40..44].copy_from_slice(&0u32.to_be_bytes()); // freeNodes
        data[44..48].copy_from_slice(&8192u32.to_be_bytes()); // clumpSize
        data[48] = 0x00; // btreeType
        data[49] = 0xCF; // keyCompareType (case-insensitive)
        data[50..54].copy_from_slice(&0x0000_0001u32.to_be_bytes()); // attributes

        let hr = HeaderRecord::parse(&data).unwrap();
        assert_eq!(hr.tree_depth, 1);
        assert_eq!(hr.root_node, 6);
        assert_eq!(hr.leaf_records, 2);
        assert_eq!(hr.first_leaf_node, 6);
        assert_eq!(hr.last_leaf_node, 6);
        assert_eq!(hr.node_size, 512);
        assert_eq!(hr.max_key_len, 516);
        assert_eq!(hr.total_nodes, 2);
        assert_eq!(hr.free_nodes, 0);
        assert_eq!(hr.clump_size, 8192);
        assert_eq!(hr.btree_type, 0x00);
        assert_eq!(hr.key_compare_type, 0xCF);
        assert_eq!(hr.attributes, 0x0000_0001);
    }

    #[test]
    fn test_header_record_case_insensitive() {
        let mut data = vec![0u8; 106];
        data[32..34].copy_from_slice(&512u16.to_be_bytes());
        data[49] = 0xCF;
        assert!(HeaderRecord::parse(&data).unwrap().is_case_insensitive());
        data[49] = 0xBC;
        assert!(!HeaderRecord::parse(&data).unwrap().is_case_insensitive());
    }

    #[test]
    fn test_header_record_too_short() {
        assert!(HeaderRecord::parse(&[0u8; 105]).is_err());
        assert!(HeaderRecord::parse(&[0u8; 0]).is_err());
    }

    #[test]
    fn test_header_record_reject_small_node_size() {
        let mut data = vec![0u8; 106];
        data[32..34].copy_from_slice(&256u16.to_be_bytes());
        assert!(HeaderRecord::parse(&data).is_err());
    }

    #[test]
    fn test_header_record_reject_non_power_of_two_node_size() {
        let mut data = vec![0u8; 106];
        data[32..34].copy_from_slice(&640u16.to_be_bytes());
        assert!(HeaderRecord::parse(&data).is_err());
    }

    #[test]
    fn test_record_offsets() {
        // Node with 2 records at offsets 14 and 94.
        // Offset table: last 2 bytes (510-511) = 14 (record 0),
        //               second-to-last 2 bytes (508-509) = 94 (record 1).
        let mut node = vec![0u8; 512];
        node[510..512].copy_from_slice(&14u16.to_be_bytes());
        node[508..510].copy_from_slice(&94u16.to_be_bytes());
        let offsets = record_offsets(&node, 2, 512);
        assert_eq!(offsets, vec![14, 94]);
    }

    #[test]
    fn test_record_offsets_empty() {
        let node = vec![0u8; 512];
        let offsets = record_offsets(&node, 0, 512);
        assert!(offsets.is_empty());
    }

    #[test]
    fn test_record_offsets_truncated_data() {
        // Node data shorter than offset table
        let node = vec![0u8; 10];
        let offsets = record_offsets(&node, 2, 512);
        assert!(offsets.is_empty());
    }

    #[test]
    fn test_read_record() {
        let node = b"abcdefghijklmnopqrstuvwxyz";
        let rec = read_record(node, 2, 5);
        assert_eq!(rec, b"cde");
    }

    #[test]
    fn test_read_record_clamped() {
        let node = b"abcdefgh";
        let rec = read_record(node, 5, 100);
        assert_eq!(rec, b"fgh");
    }

    #[test]
    fn test_read_record_zero_length() {
        let node = b"abcdefgh";
        let rec = read_record(node, 3, 3);
        assert_eq!(rec, b"");
    }
}
