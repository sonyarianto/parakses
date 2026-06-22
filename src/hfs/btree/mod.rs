pub mod key;
pub mod node;

use crate::hfs::fork::ForkReader;
use node::{HeaderRecord, NodeDescriptor, NodeType, read_record};

/// The on-disk node type mapping differs between HFS+ and HFS (original).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BTreeVariant {
    HfsPlus,
    HfsOriginal,
}

pub struct BTreeRecord {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

pub struct BTreeReader<'a> {
    fork: &'a ForkReader<'a>,
    header: HeaderRecord,
    node_size: u16,
    variant: BTreeVariant,
}

impl<'a> BTreeReader<'a> {
    pub fn open(fork: &'a ForkReader<'a>) -> anyhow::Result<Self> {
        let node0 = fork.read_range(0, fork.fork_size().min(4096))?;

        if node0.is_empty() {
            anyhow::bail!("B-tree node 0 is empty (corrupt fork)");
        }

        // Detect variant: HFS+ has HeaderNode=0, HFS original has HeaderNode=1
        let (desc, variant) = if node0[8] == 0x01 {
            // HFS original: kind=1 is HeaderNode
            (
                NodeDescriptor::parse_hfs(&node0)?,
                BTreeVariant::HfsOriginal,
            )
        } else {
            (NodeDescriptor::parse(&node0)?, BTreeVariant::HfsPlus)
        };

        if desc.kind != NodeType::HeaderNode {
            anyhow::bail!("Node 0 is not a header node (kind={:?})", desc.kind);
        }

        let header = HeaderRecord::parse(&node0)?;
        let node_size = header.node_size;

        if header.root_node == 0 && header.tree_depth > 0 {
            anyhow::bail!(
                "B-tree header has root_node=0 but tree_depth={}",
                header.tree_depth
            );
        }

        Ok(Self {
            fork,
            header,
            node_size,
            variant,
        })
    }

    pub fn header(&self) -> &HeaderRecord {
        &self.header
    }

    pub fn key_compare_type(&self) -> u8 {
        self.header.key_compare_type
    }

    pub fn is_case_insensitive(&self) -> bool {
        self.header.is_case_insensitive()
    }

    fn read_node_at(&self, index: u32) -> anyhow::Result<Vec<u8>> {
        let offset = u64::from(index) * u64::from(self.node_size);
        self.fork.read_range(offset, u64::from(self.node_size))
    }

    fn parse_node_descriptor(&self, data: &[u8]) -> anyhow::Result<NodeDescriptor> {
        match self.variant {
            BTreeVariant::HfsOriginal => NodeDescriptor::parse_hfs(data),
            BTreeVariant::HfsPlus => NodeDescriptor::parse(data),
        }
    }

    fn read_node_descriptor(&self, index: u32) -> anyhow::Result<NodeDescriptor> {
        let node_data = self.read_node_at(index)?;
        self.parse_node_descriptor(&node_data)
    }

    pub fn iter_leaf_nodes(&self) -> anyhow::Result<Vec<Vec<BTreeRecord>>> {
        let first_leaf = self.header.first_leaf_node;
        let last_leaf = self.header.last_leaf_node;

        if first_leaf == 0 {
            return Ok(Vec::new());
        }

        let mut nodes = Vec::new();
        let mut current = first_leaf;

        loop {
            let records = self.read_leaf_node(current)?;
            nodes.push(records);

            if current == last_leaf {
                break;
            }

            let desc = self.read_node_descriptor(current)?;
            current = desc.f_link;

            if current == 0 {
                break;
            }
        }

        Ok(nodes)
    }

    pub fn read_leaf_node(&self, node_index: u32) -> anyhow::Result<Vec<BTreeRecord>> {
        let node_data = self.read_node_at(node_index)?;
        let desc = self.parse_node_descriptor(&node_data)?;

        if desc.kind != NodeType::LeafNode && desc.kind != NodeType::IndexNode {
            anyhow::bail!("Node {} is not a leaf or index node", node_index);
        }

        let offsets = node::record_offsets(&node_data, desc.num_records, self.node_size);
        let mut records = Vec::with_capacity(offsets.len());

        for i in 0..offsets.len() {
            let off = offsets[i];
            let next_off = if i + 1 < offsets.len() {
                offsets[i + 1]
            } else {
                self.node_size as usize
            };

            let raw = read_record(&node_data, off, next_off);
            if raw.len() < 1 {
                continue;
            }

            let (key_data, value_data) = match self.variant {
                BTreeVariant::HfsOriginal => {
                    let ck_key_len = raw[0] as usize;
                    if ck_key_len == 0 || 1 + ck_key_len > raw.len() {
                        continue;
                    }
                    let total_key = 1 + ck_key_len;
                    let key_data = raw[2..total_key].to_vec();
                    let value_data = raw[total_key..].to_vec();
                    (key_data, value_data)
                }
                BTreeVariant::HfsPlus => {
                    if raw.len() < 2 {
                        continue;
                    }
                    let key_len = crate::util::read_u16_be(raw) as usize;
                    if key_len < 2 || key_len > raw.len() {
                        continue;
                    }
                    (raw[2..key_len].to_vec(), raw[key_len..].to_vec())
                }
            };

            records.push(BTreeRecord {
                key: key_data,
                value: value_data,
            });
        }

        Ok(records)
    }

    pub fn search_key(&self, needle: &[u8]) -> anyhow::Result<Option<BTreeRecord>> {
        self.search_node(self.header.root_node, needle)
    }

    fn search_node(&self, node_index: u32, needle: &[u8]) -> anyhow::Result<Option<BTreeRecord>> {
        if node_index == 0 {
            return Ok(None);
        }

        let node_data = self.read_node_at(node_index)?;
        let desc = self.parse_node_descriptor(&node_data)?;
        let offsets = node::record_offsets(&node_data, desc.num_records, self.node_size);

        match desc.kind {
            NodeType::LeafNode => {
                for i in 0..offsets.len() {
                    let off = offsets[i];
                    let next_off = if i + 1 < offsets.len() {
                        offsets[i + 1]
                    } else {
                        self.node_size as usize
                    };
                    let raw = read_record(&node_data, off, next_off);
                    if raw.len() < 2 {
                        continue;
                    }
                    let key_len = crate::util::read_u16_be(raw) as usize;
                    if key_len < 2 || key_len > raw.len() {
                        continue;
                    }
                    let key_data = &raw[2..key_len];

                    if key_data == needle {
                        let value = raw[key_len..].to_vec();
                        return Ok(Some(BTreeRecord {
                            key: key_data.to_vec(),
                            value,
                        }));
                    }
                }
                Ok(None)
            }
            NodeType::IndexNode => {
                for i in 0..offsets.len() {
                    let off = offsets[i];
                    let next_off = if i + 1 < offsets.len() {
                        offsets[i + 1]
                    } else {
                        self.node_size as usize
                    };
                    let raw = read_record(&node_data, off, next_off);
                    if raw.len() < 6 {
                        continue;
                    }
                    let key_len = crate::util::read_u16_be(raw) as usize;
                    if key_len < 2 {
                        continue;
                    }
                    let child_node = crate::util::read_u32_be(&raw[key_len..]);
                    let key_data = &raw[2..key_len.min(raw.len() - 4)];

                    if key_data >= needle || i == offsets.len() - 1 {
                        return self.search_node(child_node, needle);
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}
