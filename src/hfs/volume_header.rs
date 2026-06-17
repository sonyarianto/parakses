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
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
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
                data[72], data[73], data[74], data[75],
                data[76], data[77], data[78], data[79],
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
}
