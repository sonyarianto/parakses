/// Debug: dump specific bytes of the test image
fn main() {
    let data = std::fs::read("test_hfs.img").unwrap();
    
    let lba = |n: usize| { n * 512 };
    
    // Catalog header node at LBA 6
    let ch = lba(6);
    println!("=== Catalog header (LBA 6, byte {}) ===", ch);
    println!("  Bytes 508-511 (offset table): {:02X?}", &data[ch+508..ch+512]);
    println!("  Bytes 14-50 (header record): {:02X?}", &data[ch+14..ch+50]);
    
    // Parse header record manually
    let td = u16::from_be_bytes([data[ch+14], data[ch+15]]);
    let rn = u32::from_be_bytes([data[ch+16], data[ch+17], data[ch+18], data[ch+19]]);
    let lr = u32::from_be_bytes([data[ch+20], data[ch+21], data[ch+22], data[ch+23]]);
    let fl = u32::from_be_bytes([data[ch+24], data[ch+25], data[ch+26], data[ch+27]]);
    let ll = u32::from_be_bytes([data[ch+28], data[ch+29], data[ch+30], data[ch+31]]);
    let ns = u16::from_be_bytes([data[ch+32], data[ch+33]]);
    println!("  treeDepth={}, rootNode={}, leafRecords={}", td, rn, lr);
    println!("  firstLeaf={}, lastLeaf={}, nodeSize={}", fl, ll, ns);
    
    // Catalog leaf node at LBA 7
    let cl = lba(7);
    println!("\n=== Catalog leaf (LBA 7, byte {}) ===", cl);
    println!("  Node desc: fLink={:?}, bLink={:?}, kind={}, height={}, numRec={}",
        &data[cl..cl+4], &data[cl+4..cl+8], data[cl+8], data[cl+9],
        u16::from_be_bytes([data[cl+10], data[cl+11]]));
    println!("  Bytes 504-511 (offset table): {:02X?}", &data[cl+504..cl+512]);
    
    // Parse first record (root folder)
    let r1_off = 14;
    let key_len = u16::from_be_bytes([data[cl+r1_off], data[cl+r1_off+1]]);
    println!("\n  Record 1 at offset {}: keyLength={}", r1_off, key_len);
    println!("    ParentID={}, nameLen={}",
        u32::from_be_bytes([data[cl+r1_off+2], data[cl+r1_off+3], data[cl+r1_off+4], data[cl+r1_off+5]]),
        u16::from_be_bytes([data[cl+r1_off+6], data[cl+r1_off+7]]));
    
    if key_len >= 8 {
        let val_off = r1_off + key_len as usize;
        let rec_type = u16::from_be_bytes([data[cl+val_off], data[cl+val_off+1]]);
        println!("    Value record type: 0x{:04x} (Folder=1, File=2)", rec_type);
        if rec_type == 1 {
            let valence = u32::from_be_bytes([data[cl+val_off+4], data[cl+val_off+5], data[cl+val_off+6], data[cl+val_off+7]]);
            let fid = u32::from_be_bytes([data[cl+val_off+8], data[cl+val_off+9], data[cl+val_off+10], data[cl+val_off+11]]);
            println!("    Valence={}, folderID={}", valence, fid);
        }
    }
    
    // Parse second record
    let r2_off = 94; // 14 + 80
    let key_len2 = u16::from_be_bytes([data[cl+r2_off], data[cl+r2_off+1]]);
    println!("\n  Record 2 at offset {}: keyLength={}", r2_off, key_len2);
    println!("    ParentID={}, nameLen={}",
        u32::from_be_bytes([data[cl+r2_off+2], data[cl+r2_off+3], data[cl+r2_off+4], data[cl+r2_off+5]]),
        u16::from_be_bytes([data[cl+r2_off+6], data[cl+r2_off+7]]));
    
    // Print the name as UTF-16BE
    let name_len_chars = u16::from_be_bytes([data[cl+r2_off+6], data[cl+r2_off+7]]) as usize;
    if name_len_chars > 0 {
        let name_start = r2_off + 8;
        let name_bytes = &data[cl+name_start..cl+name_start + name_len_chars*2];
        let u16s: Vec<u16> = name_bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        let name = String::from_utf16_lossy(&u16s);
        println!("    Name: '{}' (bytes: {:02X?})", name, name_bytes);
    }
    
    if key_len2 >= 8 {
        let val_off2 = r2_off + key_len2 as usize;
        let rec_type2 = u16::from_be_bytes([data[cl+val_off2], data[cl+val_off2+1]]);
        println!("    Value record type: 0x{:04x}", rec_type2);
        if rec_type2 == 2 {
            let fid = u32::from_be_bytes([data[cl+val_off2+8], data[cl+val_off2+9], data[cl+val_off2+10], data[cl+val_off2+11]]);
            println!("    FileID={}", fid);
            let dsize = u64::from_be_bytes([
                data[cl+val_off2+72], data[cl+val_off2+73], data[cl+val_off2+74], data[cl+val_off2+75],
                data[cl+val_off2+76], data[cl+val_off2+77], data[cl+val_off2+78], data[cl+val_off2+79]
            ]);
            let tblks = u32::from_be_bytes([data[cl+val_off2+84], data[cl+val_off2+85], data[cl+val_off2+86], data[cl+val_off2+87]]);
            println!("    DataFork: logicalSize={}, totalBlocks={}", dsize, tblks);
        }
    }
}
