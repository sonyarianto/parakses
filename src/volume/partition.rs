use crate::blockio::BlockDevice;
use std::io;

fn read_at(device: &dyn BlockDevice, byte_offset: u64, size: usize) -> io::Result<Vec<u8>> {
    let sector_size = u64::from(device.sector_size());
    let start_lba = byte_offset / sector_size;
    let skip = (byte_offset % sector_size) as usize;
    let total_bytes = size + skip;
    let sectors_needed = total_bytes.div_ceil(sector_size as usize);
    let mut buf = Vec::with_capacity(sectors_needed * sector_size as usize);
    for i in 0..sectors_needed as u64 {
        let sector = device.read_sector(start_lba + i)?;
        buf.extend_from_slice(&sector);
    }
    Ok(buf[skip..skip + size].to_vec())
}

#[derive(Debug)]
pub enum PartitionTable {
    Mbr(Vec<MbrPartition>),
    Gpt {
        header: GptHeader,
        entries: Vec<GptEntry>,
    },
}

#[derive(Debug)]
pub struct MbrPartition {
    pub boot_indicator: u8,
    pub partition_type: u8,
    pub start_lba: u64,
    pub sector_count: u64,
}

#[derive(Debug)]
pub struct GptHeader {
    pub revision: u32,
    pub my_lba: u64,
    pub alternate_lba: u64,
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub disk_guid: uuid::Uuid,
    pub partition_entry_lba: u64,
    pub num_partition_entries: u32,
    pub partition_entry_size: u32,
}

#[derive(Debug)]
pub struct GptEntry {
    pub partition_type_guid: uuid::Uuid,
    pub unique_guid: uuid::Uuid,
    pub start_lba: u64,
    pub end_lba: u64,
    pub attributes: u64,
    pub name: String,
}

pub fn is_hfs_mbr(part_type: u8) -> bool {
    part_type == 0xAF
}

pub fn is_hfs_gpt(guid: &uuid::Uuid) -> bool {
    let apple_hfs = uuid::Uuid::from_bytes([
        0x48, 0x46, 0x53, 0x00, 0x00, 0x00, 0x11, 0xAA, 0xAA, 0x11, 0x00, 0x30, 0x65, 0x43, 0xEC,
        0xAC,
    ]);
    *guid == apple_hfs
}

pub fn detect_partition_table(
    device: &dyn BlockDevice,
    offset: u64,
) -> io::Result<Option<PartitionTable>> {
    let sector = device.read_sector(offset)?;

    if sector.len() < 512 {
        return Ok(None);
    }

    if sector[0x1FE] != 0x55 || sector[0x1FF] != 0xAA {
        return Ok(None);
    }

    let mbr_partitions = parse_mbr_entries(&sector);

    let protective_mbr = mbr_partitions.iter().any(|p| p.partition_type == 0xEE);

    if protective_mbr {
        match parse_gpt(device, offset) {
            Ok(gpt) => {
                return Ok(Some(PartitionTable::Gpt {
                    header: gpt.0,
                    entries: gpt.1,
                }));
            }
            Err(e) => {
                log::warn!(
                    "GPT header present but parse failed: {}; falling back to MBR",
                    e
                );
            }
        }
    }

    Ok(Some(PartitionTable::Mbr(mbr_partitions)))
}

fn parse_mbr_entries(data: &[u8]) -> Vec<MbrPartition> {
    let mut partitions = Vec::new();
    for i in 0..4 {
        let entry_offset = 0x1BE + i * 16;
        if entry_offset + 16 > data.len() {
            break;
        }
        let part_type = data[entry_offset + 4];
        if part_type == 0 {
            continue;
        }
        partitions.push(MbrPartition {
            boot_indicator: data[entry_offset],
            partition_type: part_type,
            start_lba: u32::from_le_bytes([
                data[entry_offset + 8],
                data[entry_offset + 9],
                data[entry_offset + 10],
                data[entry_offset + 11],
            ]) as u64,
            sector_count: u32::from_le_bytes([
                data[entry_offset + 12],
                data[entry_offset + 13],
                data[entry_offset + 14],
                data[entry_offset + 15],
            ]) as u64,
        });
    }
    partitions
}

fn parse_gpt(device: &dyn BlockDevice, offset: u64) -> io::Result<(GptHeader, Vec<GptEntry>)> {
    // GPT header is always at byte offset 512 (LBA 1 in 512-byte units)
    let sector = read_at(device, offset * 512 + 512, 512)?;

    if &sector[..8] != b"EFI PART" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid GPT signature",
        ));
    }

    let revision = u32::from_le_bytes([sector[8], sector[9], sector[10], sector[11]]);
    let header_size = u32::from_le_bytes([sector[12], sector[13], sector[14], sector[15]]);

    if header_size < 92 || header_size > sector.len() as u32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid GPT header size: {}", header_size),
        ));
    }

    let header_crc_stored = u32::from_le_bytes([sector[16], sector[17], sector[18], sector[19]]);
    if header_crc_stored != 0 {
        let mut header_copy = sector[..header_size as usize].to_vec();
        header_copy[16..20].copy_from_slice(&[0, 0, 0, 0]);
        let computed = crc32fast::hash(&header_copy);
        if computed != header_crc_stored {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "GPT header CRC mismatch: stored {:#010x}, computed {:#010x}",
                    header_crc_stored, computed
                ),
            ));
        }
    }
    let my_lba = u64::from_le_bytes([
        sector[24], sector[25], sector[26], sector[27], sector[28], sector[29], sector[30],
        sector[31],
    ]);
    let alternate_lba = u64::from_le_bytes([
        sector[32], sector[33], sector[34], sector[35], sector[36], sector[37], sector[38],
        sector[39],
    ]);
    let first_usable_lba = u64::from_le_bytes([
        sector[40], sector[41], sector[42], sector[43], sector[44], sector[45], sector[46],
        sector[47],
    ]);
    let last_usable_lba = u64::from_le_bytes([
        sector[48], sector[49], sector[50], sector[51], sector[52], sector[53], sector[54],
        sector[55],
    ]);
    let disk_guid = uuid::Uuid::from_bytes_le([
        sector[56], sector[57], sector[58], sector[59], sector[60], sector[61], sector[62],
        sector[63], sector[64], sector[65], sector[66], sector[67], sector[68], sector[69],
        sector[70], sector[71],
    ]);
    let partition_entry_lba = u64::from_le_bytes([
        sector[72], sector[73], sector[74], sector[75], sector[76], sector[77], sector[78],
        sector[79],
    ]);
    let num_partition_entries =
        u32::from_le_bytes([sector[80], sector[81], sector[82], sector[83]]);
    let partition_entry_size = u32::from_le_bytes([sector[84], sector[85], sector[86], sector[87]]);

    let header = GptHeader {
        revision,
        my_lba,
        alternate_lba,
        first_usable_lba,
        last_usable_lba,
        disk_guid,
        partition_entry_lba,
        num_partition_entries,
        partition_entry_size,
    };

    let entries = read_gpt_entries(device, &header, offset)?;

    Ok((header, entries))
}

fn read_gpt_entries(
    device: &dyn BlockDevice,
    header: &GptHeader,
    _offset: u64,
) -> io::Result<Vec<GptEntry>> {
    let entry_size = header.partition_entry_size as usize;
    let total_entries = header.num_partition_entries as usize;
    let sector_size = device.sector_size() as usize;

    let entries_per_sector = sector_size / entry_size;
    if entries_per_sector == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "GPT entry size larger than sector size",
        ));
    }

    let start_byte = header.partition_entry_lba * 512;

    let mut entries = Vec::with_capacity(total_entries);

    for i in 0..total_entries {
        let sector_idx = i / entries_per_sector;
        let entry_in_sector = i % entries_per_sector;
        let byte_off = start_byte + sector_idx as u64 * sector_size as u64;

        let sector_data = read_at(device, byte_off, sector_size)?;

        let entry_offset_in_sector = entry_in_sector * entry_size;
        if entry_offset_in_sector + entry_size > sector_data.len() {
            break;
        }
        let entry_data = &sector_data[entry_offset_in_sector..entry_offset_in_sector + entry_size];

        let type_guid = uuid::Uuid::from_bytes_le([
            entry_data[0],
            entry_data[1],
            entry_data[2],
            entry_data[3],
            entry_data[4],
            entry_data[5],
            entry_data[6],
            entry_data[7],
            entry_data[8],
            entry_data[9],
            entry_data[10],
            entry_data[11],
            entry_data[12],
            entry_data[13],
            entry_data[14],
            entry_data[15],
        ]);

        if type_guid.is_nil() {
            continue;
        }

        let unique_guid = uuid::Uuid::from_bytes_le([
            entry_data[16],
            entry_data[17],
            entry_data[18],
            entry_data[19],
            entry_data[20],
            entry_data[21],
            entry_data[22],
            entry_data[23],
            entry_data[24],
            entry_data[25],
            entry_data[26],
            entry_data[27],
            entry_data[28],
            entry_data[29],
            entry_data[30],
            entry_data[31],
        ]);

        let start_lba = u64::from_le_bytes([
            entry_data[32],
            entry_data[33],
            entry_data[34],
            entry_data[35],
            entry_data[36],
            entry_data[37],
            entry_data[38],
            entry_data[39],
        ]);
        let end_lba = u64::from_le_bytes([
            entry_data[40],
            entry_data[41],
            entry_data[42],
            entry_data[43],
            entry_data[44],
            entry_data[45],
            entry_data[46],
            entry_data[47],
        ]);
        let attributes = u64::from_le_bytes([
            entry_data[48],
            entry_data[49],
            entry_data[50],
            entry_data[51],
            entry_data[52],
            entry_data[53],
            entry_data[54],
            entry_data[55],
        ]);

        let name_utf16: Vec<u16> = entry_data[56..128]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        let name = String::from_utf16_lossy(&name_utf16);

        entries.push(GptEntry {
            partition_type_guid: type_guid,
            unique_guid,
            start_lba,
            end_lba,
            attributes,
            name,
        });
    }

    Ok(entries)
}

pub fn partition_sector_count(sector_count: u64) -> u64 {
    sector_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockio::memfile::MemFile;

    fn mbr_disk(partitions: &[(u8, u64, u64)]) -> Vec<u8> {
        let mut disk = vec![0u8; 512];
        disk[0x1FE] = 0x55;
        disk[0x1FF] = 0xAA;
        for (i, &(ptype, start, count)) in partitions.iter().enumerate() {
            let off = 0x1BE + i * 16;
            disk[off + 4] = ptype;
            disk[off + 8..off + 12].copy_from_slice(&(start as u32).to_le_bytes());
            disk[off + 12..off + 16].copy_from_slice(&(count as u32).to_le_bytes());
        }
        disk
    }

    fn make_memfile(data: Vec<u8>) -> MemFile {
        MemFile::new(data, 512)
    }

    #[test]
    fn test_detect_mbr_table() {
        let disk = mbr_disk(&[(0xAF, 1, 255)]);
        let device = make_memfile(disk.clone());
        let table = detect_partition_table(&device, 0).unwrap().unwrap();
        match table {
            PartitionTable::Mbr(parts) => {
                assert_eq!(parts.len(), 1);
                assert_eq!(parts[0].partition_type, 0xAF);
                assert_eq!(parts[0].start_lba, 1);
                assert_eq!(parts[0].sector_count, 255);
            }
            _ => panic!("Expected MBR"),
        }
    }

    #[test]
    fn test_detect_mbr_multiple() {
        let disk = mbr_disk(&[(0xAF, 1, 100), (0x0C, 101, 200), (0xAF, 301, 50)]);
        let device = make_memfile(disk);
        let table = detect_partition_table(&device, 0).unwrap().unwrap();
        match table {
            PartitionTable::Mbr(parts) => {
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0].partition_type, 0xAF);
                assert_eq!(parts[2].partition_type, 0xAF);
            }
            _ => panic!("Expected MBR"),
        }
    }

    #[test]
    fn test_detect_no_partition_table() {
        let disk = vec![0u8; 512]; // no boot signature
        let device = make_memfile(disk);
        assert!(detect_partition_table(&device, 0).unwrap().is_none());
    }

    #[test]
    fn test_is_hfs_mbr() {
        assert!(is_hfs_mbr(0xAF));
        assert!(!is_hfs_mbr(0x0C));
        assert!(!is_hfs_mbr(0x07));
        assert!(!is_hfs_mbr(0x00));
    }

    #[test]
    fn test_is_hfs_gpt() {
        let hfs_guid = uuid::Uuid::from_bytes([
            0x48, 0x46, 0x53, 0x00, 0x00, 0x00, 0x11, 0xAA, 0xAA, 0x11, 0x00, 0x30, 0x65, 0x43,
            0xEC, 0xAC,
        ]);
        assert!(is_hfs_gpt(&hfs_guid));

        let other = uuid::Uuid::nil();
        assert!(!is_hfs_gpt(&other));

        let ntfs = uuid::Uuid::from_bytes([
            0xEB, 0xD0, 0xA0, 0xA2, 0xB9, 0xE5, 0x44, 0x33, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26,
            0x99, 0xC7,
        ]);
        assert!(!is_hfs_gpt(&ntfs));
    }

    #[test]
    fn test_detect_protective_mbr_fallback_to_mbr() {
        // Protective MBR (0xEE) but no valid GPT at sector 1
        let disk = mbr_disk(&[(0xEE, 1, 255)]);
        let device = make_memfile(disk);
        let table = detect_partition_table(&device, 0).unwrap();
        assert!(table.is_some());
    }

    #[test]
    fn test_parse_gpt() {
        use uuid::Uuid;

        // Build a disk with protective MBR + valid GPT header + GPT entries
        let mut disk = vec![0u8; 2048]; // 4 sectors

        // Sector 0: protective MBR
        disk[0x1BE + 4] = 0xEE; // partition type
        disk[0x1BE + 8..0x1BE + 12].copy_from_slice(&1u32.to_le_bytes()); // start LBA = 1
        disk[0x1BE + 12..0x1BE + 16].copy_from_slice(&3u32.to_le_bytes()); // count = 3
        disk[0x1FE] = 0x55;
        disk[0x1FF] = 0xAA;

        // Sector 1: GPT header
        disk[512..520].copy_from_slice(b"EFI PART");
        disk[520..524].copy_from_slice(&0x0001_0000u32.to_le_bytes()); // revision
        disk[524..528].copy_from_slice(&92u32.to_le_bytes()); // headerSize
        disk[528..532].copy_from_slice(&0u32.to_le_bytes()); // crc32 (zero = skip check)
        disk[536..544].copy_from_slice(&1u64.to_le_bytes()); // myLBA
        disk[544..552].copy_from_slice(&3u64.to_le_bytes()); // alternateLBA
        disk[552..560].copy_from_slice(&34u64.to_le_bytes()); // firstUsableLBA
        disk[560..568].copy_from_slice(&100u64.to_le_bytes()); // lastUsableLBA
        // diskGUID at offset 56 (sector 1 + 56 = byte 568)
        let guid = Uuid::parse_str("A1B2C3D4-E5F6-7890-ABCD-EF1234567890").unwrap();
        let guid_bytes = guid.to_bytes_le();
        disk[568..584].copy_from_slice(&guid_bytes);
        disk[584..592].copy_from_slice(&2u64.to_le_bytes()); // partitionEntryLBA
        disk[592..596].copy_from_slice(&4u32.to_le_bytes()); // numPartitionEntries (only 1 sector worth fits in 4-sector disk)
        disk[596..600].copy_from_slice(&128u32.to_le_bytes()); // partitionEntrySize

        let hfs_guid = uuid::Uuid::from_bytes([
            0x48, 0x46, 0x53, 0x00, 0x00, 0x00, 0x11, 0xAA, 0xAA, 0x11, 0x00, 0x30, 0x65, 0x43,
            0xEC, 0xAC,
        ]);

        // Sector 2: first partition entry
        let entry_off = 1024;
        let hfs_guid_bytes = hfs_guid.to_bytes_le();
        disk[entry_off..entry_off + 16].copy_from_slice(&hfs_guid_bytes); // partition type GUID
        disk[entry_off + 16..entry_off + 32].copy_from_slice(&uuid::Uuid::nil().to_bytes_le()); // unique GUID
        disk[entry_off + 32..entry_off + 40].copy_from_slice(&40u64.to_le_bytes()); // startLBA
        disk[entry_off + 40..entry_off + 48].copy_from_slice(&100u64.to_le_bytes()); // endLBA
        // name at offset 56
        let name = "TestHFS";
        let name_utf16: Vec<u16> = name.encode_utf16().collect();
        for (i, &c) in name_utf16.iter().take(36).enumerate() {
            let off = entry_off + 56 + i * 2;
            disk[off..off + 2].copy_from_slice(&c.to_le_bytes());
        }

        let device = make_memfile(disk.to_vec());
        let table = detect_partition_table(&device, 0).unwrap().unwrap();
        match table {
            PartitionTable::Gpt { header, entries } => {
                assert_eq!(header.revision, 0x0001_0000);
                assert_eq!(header.my_lba, 1);
                assert_eq!(entries.len(), 1);
                assert!(is_hfs_gpt(&entries[0].partition_type_guid));
                assert_eq!(entries[0].start_lba, 40);
                assert_eq!(entries[0].end_lba, 100);
                assert_eq!(entries[0].name, "TestHFS");
            }
            _ => panic!("Expected GPT"),
        }
    }

    #[test]
    fn test_parse_mbr_entries_empty() {
        let disk = vec![0u8; 512];
        let parts = parse_mbr_entries(&disk);
        assert!(parts.is_empty());
    }

    #[test]
    fn test_detect_table_bad_signature() {
        let mut disk = vec![0u8; 512];
        disk[0x1FE] = 0x55;
        disk[0x1FF] = 0xAA;
        disk[0x1BE + 4] = 0xAF;
        let device = make_memfile(disk);
        let table = detect_partition_table(&device, 0).unwrap();
        assert!(table.is_some());
    }

    #[test]
    fn test_too_short_sector() {
        let disk = vec![0u8; 10];
        let device = MemFile::new(disk, 512);
        assert!(detect_partition_table(&device, 0).unwrap().is_none());
    }
}
