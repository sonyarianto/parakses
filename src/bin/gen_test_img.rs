use std::fs;
use std::path::Path;

const BLOCK_SIZE: u32 = 512;
const PART_SECTOR: u64 = 1; // partition starts at LBA 1
const PART_BYTE: u64 = PART_SECTOR * 512;
const TOTAL_BLOCKS: u32 = 256;
const FIRST_ALLOC: u32 = 3; // allocation block 3 is the first allocatable

fn be16(v: u16) -> [u8; 2] { v.to_be_bytes() }
fn be32(v: u32) -> [u8; 4] { v.to_be_bytes() }
fn be64(v: u64) -> [u8; 8] { v.to_be_bytes() }
fn le32(v: u32) -> [u8; 4] { v.to_le_bytes() }

/// Convert an allocation block number to an absolute byte offset in the disk image.
fn alloc_block_byte(alloc_block: u32) -> usize {
    (PART_BYTE + u64::from(alloc_block) * u64::from(BLOCK_SIZE)) as usize
}

fn write_extent(disk: &mut [u8], off: usize, start: u32, count: u32) {
    disk[off..off+4].copy_from_slice(&be32(start));
    disk[off+4..off+8].copy_from_slice(&be32(count));
}

fn build_mbr(disk: &mut [u8]) {
    let off = 0x1BE;
    disk[off+4] = 0xAF; // Apple HFS+
    disk[off+8..off+12].copy_from_slice(&le32(1)); // start LBA
    disk[off+12..off+16].copy_from_slice(&le32(TOTAL_BLOCKS - 1)); // sector count
    disk[0x1FE] = 0x55;
    disk[0x1FF] = 0xAA;
}

fn build_volume_header(disk: &mut [u8]) {
    // Volume Header is at partition byte 1024 (offset within partition)
    let off = PART_BYTE as usize + 1024;

    disk[off..off+2].copy_from_slice(&be16(0x482B)); // "H+" signature
    disk[off+2..off+4].copy_from_slice(&be16(4)); // version
    disk[off+40..off+44].copy_from_slice(&be32(BLOCK_SIZE));
    disk[off+44..off+48].copy_from_slice(&be32(TOTAL_BLOCKS));
    disk[off+48..off+52].copy_from_slice(&be32(TOTAL_BLOCKS - 10));
    disk[off+52..off+56].copy_from_slice(&be32(9)); // nextAllocation
    disk[off+64..off+68].copy_from_slice(&be32(10)); // nextCatalogID
    disk[off+68..off+72].copy_from_slice(&be32(1)); // writeCount
    disk[off+32..off+36].copy_from_slice(&be32(1)); // fileCount
    disk[off+36..off+40].copy_from_slice(&be32(1)); // folderCount

    // Special file extent descriptors (allocation block numbers)
    write_extent(disk, off+112, FIRST_ALLOC, 1); // allocationFile
    write_extent(disk, off+120, FIRST_ALLOC+1, 1); // extentsFile
    write_extent(disk, off+128, FIRST_ALLOC+2, 2); // catalogFile
    write_extent(disk, off+136, FIRST_ALLOC+4, 1); // attributesFile
    write_extent(disk, off+144, 0, 0); // startupFile
    disk[off+156..off+160].copy_from_slice(&be32(FIRST_ALLOC)); // firstAllocationBlock
}

fn build_alloc_bitmap(disk: &mut [u8]) {
    let off = alloc_block_byte(FIRST_ALLOC);
    // Blocks 0-7: all used (Volume Header area + special files)
    disk[off] = 0xFF;
    // Block 8: used for sample file data
    disk[off+1] = 0x01;
}

fn btree_header_node(
    disk: &mut [u8], alloc_block: u32,
    tree_depth: u16, root_node: u32, leaf_records: u32,
    first_leaf: u32, last_leaf: u32, total_nodes: u32,
    btree_type: u8, key_compare_type: u8,
) {
    let off = alloc_block_byte(alloc_block);
    disk[off+10..off+12].copy_from_slice(&be16(1)); // numRecords = 1
    disk[off+8] = 0; // kind = header
    let hr = off + 14;
    disk[hr..hr+2].copy_from_slice(&be16(tree_depth));
    disk[hr+2..hr+6].copy_from_slice(&be32(root_node));
    disk[hr+6..hr+10].copy_from_slice(&be32(leaf_records));
    disk[hr+10..hr+14].copy_from_slice(&be32(first_leaf));
    disk[hr+14..hr+18].copy_from_slice(&be32(last_leaf));
    disk[hr+18..hr+20].copy_from_slice(&be16(BLOCK_SIZE as u16));
    disk[hr+20..hr+22].copy_from_slice(&be16(516));
    disk[hr+22..hr+26].copy_from_slice(&be32(total_nodes));
    disk[hr+30..hr+34].copy_from_slice(&be32(8192));
    disk[hr+34] = btree_type;
    disk[hr+35] = key_compare_type;
    let ot = off + BLOCK_SIZE as usize - 2;
    disk[ot..ot+2].copy_from_slice(&be16(14)); // header record at offset 14
}

fn catalog_leaf_node(disk: &mut [u8], alloc_block: u32) {
    let node_off = alloc_block_byte(alloc_block);

    // Node descriptor
    disk[node_off+10..node_off+12].copy_from_slice(&be16(2)); // numRecords = 2
    disk[node_off+8] = 2; // kind = leaf
    disk[node_off+9] = 1; // height = 1

    // Record 1: Root folder (parentID=1, name="")
    let r1_off = node_off + 14;
    disk[r1_off..r1_off+2].copy_from_slice(&be16(8)); // keyLength
    disk[r1_off+2..r1_off+6].copy_from_slice(&be32(1)); // parentID
    disk[r1_off+6..r1_off+8].copy_from_slice(&be16(0)); // nameLength
    let v1 = r1_off + 8;
    disk[v1..v1+2].copy_from_slice(&be16(1)); // recordType = Folder
    disk[v1+4..v1+8].copy_from_slice(&be32(1)); // valence = 1 child
    disk[v1+8..v1+12].copy_from_slice(&be32(1)); // folderID = kHFSRootFolderID
    // rest of folder record is zeros (dates, etc.)
    // textEncoding at v1+64, folderCount at v1+68

    // Record 2: "hello.txt" (parentID=1)
    let name = "hello.txt";
    let name_utf16: Vec<u16> = name.encode_utf16().collect();
    let name_bytes = name_utf16.len() * 2;
    let key_len = 8 + name_bytes; // keyLength field + parentID + nameLen + name
    let r2_off = r1_off + 80; // 8 key + 72 value

    // Key
    disk[r2_off..r2_off+2].copy_from_slice(&be16(key_len as u16));
    disk[r2_off+2..r2_off+6].copy_from_slice(&be32(1)); // parentID = root
    disk[r2_off+6..r2_off+8].copy_from_slice(&be16(name_utf16.len() as u16));
    for (i, &c) in name_utf16.iter().enumerate() {
        disk[r2_off+8+i*2..r2_off+10+i*2].copy_from_slice(&be16(c));
    }

    // Value: FileRecord
    let v2 = r2_off + key_len as usize;
    disk[v2..v2+2].copy_from_slice(&be16(2)); // recordType = kHFSPlusFileRecord
    disk[v2+8..v2+12].copy_from_slice(&be32(2)); // fileID
    // Data fork (80 bytes at offset 72)
    disk[v2+72..v2+80].copy_from_slice(&be64(29)); // logicalSize
    disk[v2+84..v2+88].copy_from_slice(&be32(1)); // totalBlocks
    write_extent(disk, v2+88, FIRST_ALLOC+5, 1); // data at alloc block 8

    // Record offset table (2 entries at the end of the node)
    let ot_base = node_off + BLOCK_SIZE as usize - 4;
    disk[ot_base..ot_base+2].copy_from_slice(&be16(14)); // record 1 at offset 14
    disk[ot_base+2..ot_base+4].copy_from_slice(&be16((r2_off - node_off) as u16));
}

fn build_file_content(disk: &mut [u8]) {
    let content = b"Hello from test HFS+ volume!\n";
    let alloc_block = FIRST_ALLOC + 5; // alloc block 8
    let off = alloc_block_byte(alloc_block);
    disk[off..off+content.len()].copy_from_slice(content);
}

fn main() -> anyhow::Result<()> {
    let out_path = Path::new("test_hfs.img");
    let disk_size = u64::from(TOTAL_BLOCKS) * u64::from(BLOCK_SIZE);
    let mut disk = vec![0u8; disk_size as usize];

    build_mbr(&mut disk);
    build_volume_header(&mut disk);
    build_alloc_bitmap(&mut disk);

    // Extents B-tree header at alloc block 4
    btree_header_node(&mut disk, FIRST_ALLOC+1, 0, 0, 0, 0, 0, 1, 0x02, 0xBC);
    // Attributes B-tree header at alloc block 7
    btree_header_node(&mut disk, FIRST_ALLOC+4, 0, 0, 0, 0, 0, 1, 0x03, 0xBC);
    // Catalog B-tree header at alloc block 5
    // Node indices within the fork: node 0 = alloc block 5 (header), node 1 = alloc block 6 (leaf)
    btree_header_node(&mut disk, FIRST_ALLOC+2, 1, 1, 2,
        1, 1, 2, 0x00, 0xCF);
    // Catalog leaf node at alloc block 6
    catalog_leaf_node(&mut disk, FIRST_ALLOC+3);

    // File content at alloc block 8
    build_file_content(&mut disk);

    fs::write(out_path, &disk)?;

    println!("Generated {}", out_path.display());
    println!("  Size: {} bytes ({} KB)", disk_size, disk_size / 1024);
    println!("  Partition: LBA {}, Type: Apple HFS+ (0xAF)", PART_SECTOR);
    println!();
    println!("Usage:");
    println!("  cargo run --bin parakses -- --image {} volumes", out_path.display());
    println!("  cargo run --bin parakses -- --image {} list 0 /", out_path.display());
    println!("  cargo run --bin parakses -- --image {} cat 0 /hello.txt", out_path.display());
    println!("  cargo run --bin parakses -- --image {} extract 0 /hello.txt out.txt", out_path.display());

    Ok(())
}
