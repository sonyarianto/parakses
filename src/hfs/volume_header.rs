use crate::util::{read_u16_be, read_u32_be};

#[derive(Debug, Clone)]
pub struct HfsPlusExtentDescriptor {
    pub start_block: u32,
    pub block_count: u32,
}

impl HfsPlusExtentDescriptor {
    pub fn parse(data: &[u8]) -> Self {
        Self {
            start_block: read_u32_be(data),
            block_count: read_u32_be(&data[4..]),
        }
    }
}

pub const HFS_PLUS_EXTENT_DESCRIPTOR_SIZE: usize = 8;
pub const HFS_PLUS_EXTENT_RECORD_COUNT: usize = 8;

/// Parse 8 extents from fork data (starting at offset in data).
pub fn parse_extent_record(data: &[u8]) -> Vec<HfsPlusExtentDescriptor> {
    let mut extents = Vec::with_capacity(HFS_PLUS_EXTENT_RECORD_COUNT);
    for i in 0..HFS_PLUS_EXTENT_RECORD_COUNT {
        let off = i * HFS_PLUS_EXTENT_DESCRIPTOR_SIZE;
        if off + HFS_PLUS_EXTENT_DESCRIPTOR_SIZE <= data.len() {
            extents.push(HfsPlusExtentDescriptor::parse(&data[off..]));
        }
    }
    extents
}

// HFSPlusForkData layout (80 bytes):
//   0: logicalSize  (u64 BE)
//   8: clumpSize    (u32 BE)
//  12: totalBlocks  (u32 BE)
//  16: extents[8]   (8 * 8 = 64 bytes)
#[derive(Debug, Clone)]
pub struct HfsPlusForkData {
    pub logical_size: u64,
    pub clump_size: u32,
    pub total_blocks: u32,
    pub extents: Vec<HfsPlusExtentDescriptor>,
}

impl HfsPlusForkData {
    pub fn parse(data: &[u8]) -> Self {
        let logical_size = u64::from_be_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);
        let clump_size = read_u32_be(&data[8..]);
        let total_blocks = read_u32_be(&data[12..]);
        let extents = parse_extent_record(&data[16..]);
        Self {
            logical_size,
            clump_size,
            total_blocks,
            extents,
        }
    }

    pub fn total_inline_blocks(&self) -> u32 {
        self.extents.iter().map(|e| e.block_count).sum()
    }

    pub fn needs_overflow(&self) -> bool {
        self.total_blocks > self.total_inline_blocks()
    }
}

#[derive(Debug)]
pub struct VolumeHeader {
    pub signature: u16,
    pub version: u16,
    pub attributes: u32,
    pub last_mounted_version: u32,
    pub journal_info_block: u32,
    pub create_date: u32,
    pub modify_date: u32,
    pub backup_date: u32,
    pub checked_date: u32,
    pub file_count: u32,
    pub folder_count: u32,
    pub block_size: u32,
    pub total_blocks: u32,
    pub free_blocks: u32,
    pub next_allocation: u32,
    pub rsrc_clump_size: u32,
    pub data_clump_size: u32,
    pub next_catalog_id: u32,
    pub write_count: u32,
    pub encodings_bitmap: u64,
    pub finder_info: [u32; 8],
    pub allocation_file: HfsPlusExtentDescriptor,
    pub extents_file: HfsPlusExtentDescriptor,
    pub catalog_file: HfsPlusExtentDescriptor,
    pub attributes_file: HfsPlusExtentDescriptor,
    pub startup_file: HfsPlusExtentDescriptor,
    pub btree_nodes: u32,
    pub first_allocation_block: u32,
}

impl VolumeHeader {
    pub fn parse(data: &[u8]) -> anyhow::Result<Self> {
        if data.len() < 512 {
            anyhow::bail!("Volume header too short: {} bytes", data.len());
        }

        let signature = read_u16_be(&data[0..]);
        if signature != 0x482B && signature != 0x4858 {
            anyhow::bail!("Not an HFS+ volume (signature: {:#06x})", signature);
        }

        let version = read_u16_be(&data[2..]);
        let block_size = read_u32_be(&data[40..]);
        let total_blocks = read_u32_be(&data[44..]);

        if block_size < 512 || !block_size.is_power_of_two() {
            anyhow::bail!(
                "Invalid HFS+ block size: {} (must be power of two, >= 512)",
                block_size
            );
        }

        if total_blocks == 0 {
            anyhow::bail!("Volume has zero blocks (corrupt header)");
        }

        Ok(Self {
            signature,
            version,
            attributes: read_u32_be(&data[4..]),
            last_mounted_version: read_u32_be(&data[8..]),
            journal_info_block: read_u32_be(&data[12..]),
            create_date: read_u32_be(&data[16..]),
            modify_date: read_u32_be(&data[20..]),
            backup_date: read_u32_be(&data[24..]),
            checked_date: read_u32_be(&data[28..]),
            file_count: read_u32_be(&data[32..]),
            folder_count: read_u32_be(&data[36..]),
            block_size,
            total_blocks,
            free_blocks: read_u32_be(&data[48..]),
            next_allocation: read_u32_be(&data[52..]),
            rsrc_clump_size: read_u32_be(&data[56..]),
            data_clump_size: read_u32_be(&data[60..]),
            next_catalog_id: read_u32_be(&data[64..]),
            write_count: read_u32_be(&data[68..]),
            encodings_bitmap: u64::from_be_bytes([
                data[72], data[73], data[74], data[75], data[76], data[77], data[78], data[79],
            ]),
            finder_info: [
                read_u32_be(&data[80..]),
                read_u32_be(&data[84..]),
                read_u32_be(&data[88..]),
                read_u32_be(&data[92..]),
                read_u32_be(&data[96..]),
                read_u32_be(&data[100..]),
                read_u32_be(&data[104..]),
                read_u32_be(&data[108..]),
            ],
            allocation_file: HfsPlusExtentDescriptor::parse(&data[112..]),
            extents_file: HfsPlusExtentDescriptor::parse(&data[120..]),
            catalog_file: HfsPlusExtentDescriptor::parse(&data[128..]),
            attributes_file: HfsPlusExtentDescriptor::parse(&data[136..]),
            startup_file: HfsPlusExtentDescriptor::parse(&data[144..]),
            btree_nodes: read_u32_be(&data[152..]),
            first_allocation_block: read_u32_be(&data[156..]),
        })
    }

    pub fn is_journaled(&self) -> bool {
        self.journal_info_block != 0
    }

    pub fn is_hfs_plus(&self) -> bool {
        self.signature == 0x482B
    }

    pub fn is_hfsx(&self) -> bool {
        self.signature == 0x4858
    }

    pub fn volume_name(&self) -> &str {
        "Untitled"
    }

    pub fn is_hfs_original(&self) -> bool {
        self.signature == 0x4242
    }
}

/// HFS (original) Master Directory Block — parsed from the first 162 bytes at offset 1024.
#[derive(Debug)]
pub struct HfsMdb {
    pub signature: u16,         // drSigWord — should be 0x4242 (BD)
    pub num_files_root: u16,    // drNmFls — files in root directory
    pub vbm_start: u16,         // drVBMSt — first block of VBM
    pub alloc_ptr: u16,         // drAllocPtr
    pub num_alloc_blocks: u16,  // drNmAlBlks
    pub alloc_block_size: u32,  // drAlBlkSiz — always 512 for HFS
    pub clump_size: u32,        // drClpSiz
    pub first_alloc_block: u16, // drAlBlSt — first allocation block number
    pub next_cnid: u32,         // drNxtCNID
    pub free_blocks: u16,       // drFreeBks
    pub volume_name: String,    // drVN (pascal string, max 27 bytes)
    pub write_count: u32,       // drWrCnt
    pub file_count: u32,        // drFilCnt
    pub folder_count: u32,      // drDirCnt
    pub xt_fl_size: u32,        // drXTFlSize — extents B-tree logical size
    pub ct_fl_size: u32,        // drCTFlSize — catalog B-tree logical size
    // Extents B-tree first extent record (3 HFSExtentDescriptor: each U16 start, U16 count)
    pub xt_extents: [HfsExtentDescriptor; 3],
    // Catalog B-tree first extent record (3 HFSExtentDescriptor)
    pub ct_extents: [HfsExtentDescriptor; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct HfsExtentDescriptor {
    pub start_block: u16,
    pub block_count: u16,
}

impl HfsMdb {
    pub fn parse(data: &[u8]) -> anyhow::Result<Self> {
        if data.len() < 162 {
            anyhow::bail!("MDB too short: {} bytes", data.len());
        }
        let signature = read_u16_be(&data[0..]);
        if signature != 0x4244 {
            anyhow::bail!("Not an HFS volume (signature: {:#06x})", signature);
        }

        let num_files_root = read_u16_be(&data[12..]);
        let vbm_start = read_u16_be(&data[14..]);
        let alloc_ptr = read_u16_be(&data[16..]);
        let num_alloc_blocks = read_u16_be(&data[18..]);
        let alloc_block_size = read_u32_be(&data[20..]);
        let clump_size = read_u32_be(&data[24..]);
        let first_alloc_block = read_u16_be(&data[28..]);
        let next_cnid = read_u32_be(&data[30..]);
        let free_blocks = read_u16_be(&data[34..]);

        let name_len = data[36] as usize;
        let name_bytes = if name_len > 27 { 27 } else { name_len };
        let volume_name = String::from_utf8_lossy(&data[37..37 + name_bytes]).to_string();

        let write_count = read_u32_be(&data[70..]);
        let file_count = read_u32_be(&data[84..]);
        let folder_count = read_u32_be(&data[88..]);

        let xt_fl_size = read_u32_be(&data[130..]);
        let xt_extents = Self::parse_hfs_extent_record(&data[134..]);

        let ct_fl_size = read_u32_be(&data[146..]);
        let ct_extents = Self::parse_hfs_extent_record(&data[150..]);

        Ok(Self {
            signature,
            num_files_root,
            vbm_start,
            alloc_ptr,
            num_alloc_blocks,
            alloc_block_size,
            clump_size,
            first_alloc_block,
            next_cnid,
            free_blocks,
            volume_name,
            write_count,
            file_count,
            folder_count,
            xt_fl_size,
            ct_fl_size,
            xt_extents,
            ct_extents,
        })
    }

    fn parse_hfs_extent_record(data: &[u8]) -> [HfsExtentDescriptor; 3] {
        let mut extents = [HfsExtentDescriptor {
            start_block: 0,
            block_count: 0,
        }; 3];
        for i in 0..3 {
            let off = i * 4;
            if off + 4 <= data.len() {
                extents[i] = HfsExtentDescriptor {
                    start_block: read_u16_be(&data[off..]),
                    block_count: read_u16_be(&data[off + 2..]),
                };
            }
        }
        extents
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_vh_data() -> Vec<u8> {
        let mut d = vec![0u8; 512];
        d[0..2].copy_from_slice(&0x482Bu16.to_be_bytes());
        d[2..4].copy_from_slice(&4u16.to_be_bytes());
        d[40..44].copy_from_slice(&512u32.to_be_bytes());
        d[44..48].copy_from_slice(&256u32.to_be_bytes());
        d[48..52].copy_from_slice(&10u32.to_be_bytes());
        d
    }

    #[test]
    fn test_parse_hfs_plus() {
        let vh = VolumeHeader::parse(&valid_vh_data()).unwrap();
        assert_eq!(vh.signature, 0x482B);
        assert_eq!(vh.version, 4);
        assert!(vh.is_hfs_plus());
        assert!(!vh.is_hfsx());
        assert_eq!(vh.block_size, 512);
        assert_eq!(vh.total_blocks, 256);
        assert_eq!(vh.free_blocks, 10);
    }

    #[test]
    fn test_parse_hfsx() {
        let mut d = valid_vh_data();
        d[0..2].copy_from_slice(&0x4858u16.to_be_bytes());
        let vh = VolumeHeader::parse(&d).unwrap();
        assert!(vh.is_hfsx());
        assert!(!vh.is_hfs_plus());
    }

    #[test]
    fn test_reject_bad_signature() {
        let mut d = valid_vh_data();
        d[0..2].copy_from_slice(&0x1234u16.to_be_bytes());
        assert!(VolumeHeader::parse(&d).is_err());
    }

    #[test]
    fn test_reject_block_size_below_512() {
        let mut d = valid_vh_data();
        d[40..44].copy_from_slice(&256u32.to_be_bytes());
        assert!(VolumeHeader::parse(&d).is_err());
    }

    #[test]
    fn test_reject_block_size_not_power_of_two() {
        let mut d = valid_vh_data();
        d[40..44].copy_from_slice(&1024u32.to_be_bytes()); // valid
        let vh = VolumeHeader::parse(&d);
        assert!(vh.is_ok());

        let mut d = valid_vh_data();
        d[40..44].copy_from_slice(&640u32.to_be_bytes());
        assert!(VolumeHeader::parse(&d).is_err());
    }

    #[test]
    fn test_reject_zero_blocks() {
        let mut d = valid_vh_data();
        d[44..48].copy_from_slice(&0u32.to_be_bytes());
        assert!(VolumeHeader::parse(&d).is_err());
    }

    #[test]
    fn test_extent_descriptor_parse() {
        let data = [0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x02];
        let ext = HfsPlusExtentDescriptor::parse(&data);
        assert_eq!(ext.start_block, 5);
        assert_eq!(ext.block_count, 2);
    }

    #[test]
    fn test_extent_descriptor_zero() {
        let data = [0u8; 8];
        let ext = HfsPlusExtentDescriptor::parse(&data);
        assert_eq!(ext.start_block, 0);
        assert_eq!(ext.block_count, 0);
    }

    #[test]
    fn test_fork_data_parse() {
        let mut d = vec![0u8; 80];
        d[0..8].copy_from_slice(&29u64.to_be_bytes());
        d[8..12].copy_from_slice(&8192u32.to_be_bytes());
        d[12..16].copy_from_slice(&1u32.to_be_bytes());
        d[16..24].copy_from_slice(&[0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01]); // 1 extent
        let fork = HfsPlusForkData::parse(&d);
        assert_eq!(fork.logical_size, 29);
        assert_eq!(fork.clump_size, 8192);
        assert_eq!(fork.total_blocks, 1);
        assert_eq!(fork.extents.len(), 8);
        assert_eq!(fork.extents[0].start_block, 8);
        assert_eq!(fork.extents[0].block_count, 1);
        assert_eq!(fork.total_inline_blocks(), 1);
        assert!(!fork.needs_overflow());
    }

    #[test]
    fn test_fork_needs_overflow() {
        let mut d = vec![0u8; 80];
        d[12..16].copy_from_slice(&10u32.to_be_bytes()); // total_blocks = 10
        d[16..24].copy_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01]); // 1 extent, 1 block
        let fork = HfsPlusForkData::parse(&d);
        assert_eq!(fork.total_blocks, 10);
        assert_eq!(fork.total_inline_blocks(), 1);
        assert!(fork.needs_overflow());
    }

    #[test]
    fn test_is_journaled() {
        let mut d = valid_vh_data();
        let vh = VolumeHeader::parse(&d).unwrap();
        assert!(!vh.is_journaled());
        d[12..16].copy_from_slice(&42u32.to_be_bytes());
        let vh = VolumeHeader::parse(&d).unwrap();
        assert!(vh.is_journaled());
    }

    #[test]
    fn test_volume_name() {
        let d = valid_vh_data();
        let vh = VolumeHeader::parse(&d).unwrap();
        assert_eq!(vh.volume_name(), "Untitled");
    }

    #[test]
    fn test_vh_fields() {
        let mut d = valid_vh_data();
        d[4..8].copy_from_slice(&0x8000_0001u32.to_be_bytes()); // attributes
        d[8..12].copy_from_slice(&0x0001_0002u32.to_be_bytes()); // lastMountedVersion
        d[16..20].copy_from_slice(&100_000u32.to_be_bytes()); // createDate
        d[32..36].copy_from_slice(&5u32.to_be_bytes()); // fileCount
        d[36..40].copy_from_slice(&3u32.to_be_bytes()); // folderCount
        d[52..56].copy_from_slice(&42u32.to_be_bytes()); // nextAllocation
        d[64..68].copy_from_slice(&100u32.to_be_bytes()); // nextCatalogID
        d[68..72].copy_from_slice(&7u32.to_be_bytes()); // writeCount

        let vh = VolumeHeader::parse(&d).unwrap();
        assert_eq!(vh.attributes, 0x8000_0001);
        assert_eq!(vh.last_mounted_version, 0x0001_0002);
        assert_eq!(vh.create_date, 100_000);
        assert_eq!(vh.file_count, 5);
        assert_eq!(vh.folder_count, 3);
        assert_eq!(vh.next_allocation, 42);
        assert_eq!(vh.next_catalog_id, 100);
        assert_eq!(vh.write_count, 7);
    }

    #[test]
    fn test_vh_special_files() {
        let mut d = valid_vh_data();
        // allocationFile at offset 112
        d[112..120].copy_from_slice(&[0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01]);
        // extentsFile at offset 120
        d[120..128].copy_from_slice(&[0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01]);
        // catalogFile at offset 128
        d[128..136].copy_from_slice(&[0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x02]);
        // attributesFile at offset 136
        d[136..144].copy_from_slice(&[0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x01]);

        let vh = VolumeHeader::parse(&d).unwrap();
        assert_eq!(vh.allocation_file.start_block, 3);
        assert_eq!(vh.allocation_file.block_count, 1);
        assert_eq!(vh.extents_file.start_block, 4);
        assert_eq!(vh.extents_file.block_count, 1);
        assert_eq!(vh.catalog_file.start_block, 5);
        assert_eq!(vh.catalog_file.block_count, 2);
        assert_eq!(vh.attributes_file.start_block, 7);
        assert_eq!(vh.attributes_file.block_count, 1);
    }

    #[test]
    fn test_parse_extent_record() {
        let mut data = vec![0u8; 8 * 8];
        data[0..8].copy_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02]);
        data[8..16].copy_from_slice(&[0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04]);
        let extents = parse_extent_record(&data);
        assert_eq!(extents.len(), 8);
        assert_eq!(extents[0].start_block, 1);
        assert_eq!(extents[0].block_count, 2);
        assert_eq!(extents[1].start_block, 3);
        assert_eq!(extents[1].block_count, 4);
        // remaining 6 extents should be zero
        for i in 2..8 {
            assert_eq!(extents[i].start_block, 0);
            assert_eq!(extents[i].block_count, 0);
        }
    }

    #[test]
    fn test_vh_too_short() {
        let d = vec![0u8; 100];
        assert!(VolumeHeader::parse(&d).is_err());
    }
}
