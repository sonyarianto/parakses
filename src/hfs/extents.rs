use crate::hfs::btree::BTreeReader;
use crate::hfs::volume_header::{
    parse_extent_record, HfsPlusExtentDescriptor, HFS_PLUS_EXTENT_DESCRIPTOR_SIZE,
};
use crate::util::read_u32_be;

pub struct ExtentsOverflowReader<'a> {
    tree: BTreeReader<'a>,
}

impl<'a> ExtentsOverflowReader<'a> {
    pub fn open(tree: BTreeReader<'a>) -> Self {
        Self { tree }
    }

    /// Look up extents for a given file ID and fork type.
    /// Searches for an exact match on (file_id, fork_type, start_block).
    pub fn lookup_extents(
        &self,
        file_id: u32,
        fork_type: u8,
        start_block: u32,
    ) -> anyhow::Result<Vec<HfsPlusExtentDescriptor>> {
        let needle = Self::encode_key(file_id, fork_type, start_block);
        let record = self.tree.search_key(&needle)?;

        match record {
            Some(rec) => {
                let extents = parse_extent_record(&rec.value);
                Ok(extents.into_iter().filter(|e| e.block_count > 0).collect())
            }
            None => Ok(Vec::new()),
        }
    }

    /// Look up ALL overflow extents for a file + fork type by scanning leaf nodes.
    pub fn lookup_all_extents(
        &self,
        file_id: u32,
        fork_type: u8,
    ) -> anyhow::Result<Vec<HfsPlusExtentDescriptor>> {
        let leaf_nodes = self.tree.iter_leaf_nodes()?;
        let mut results = Vec::new();

        for node_records in &leaf_nodes {
            for rec in node_records {
                if rec.key.len() < 5 {
                    continue;
                }
                let rec_file_id = read_u32_be(&rec.key[..4]);
                let rec_fork_type = rec.key[4];
                if rec_file_id != file_id || rec_fork_type != fork_type {
                    continue;
                }
                if rec.value.len() < 8 * HFS_PLUS_EXTENT_DESCRIPTOR_SIZE {
                    log::warn!("Short extent record for file {} fork {}: {} bytes", file_id, fork_type, rec.value.len());
                    continue;
                }
                let extents = parse_extent_record(&rec.value);
                for ext in extents {
                    if ext.block_count > 0 {
                        results.push(ext);
                    }
                }
            }
        }

        Ok(results)
    }

    fn encode_key(file_id: u32, fork_type: u8, start_block: u32) -> Vec<u8> {
        let mut buf = Vec::with_capacity(9);
        buf.extend_from_slice(&file_id.to_be_bytes());
        buf.push(fork_type);
        buf.extend_from_slice(&start_block.to_be_bytes());
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_key_format() {
        let key = ExtentsOverflowReader::encode_key(42, 0, 7);
        assert_eq!(key.len(), 9);
        assert_eq!(&key[..4], &42u32.to_be_bytes());
        assert_eq!(key[4], 0);
        assert_eq!(&key[5..], &7u32.to_be_bytes());
    }

    #[test]
    fn test_encode_key_resource_fork() {
        let key = ExtentsOverflowReader::encode_key(1, 1, 0);
        assert_eq!(key[4], 1);
    }

    #[test]
    fn test_encode_key_large_values() {
        let key = ExtentsOverflowReader::encode_key(0xFFFFFFFF, 0xFF, 0xFFFFFFFF);
        assert_eq!(key.len(), 9);
        assert_eq!(&key[..4], &0xFFFFFFFFu32.to_be_bytes());
        assert_eq!(key[4], 0xFF);
        assert_eq!(&key[5..], &0xFFFFFFFFu32.to_be_bytes());
    }
}
