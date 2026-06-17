pub mod date;

/// Read a big-endian u16 from a byte slice.
pub fn read_u16_be(data: &[u8]) -> u16 {
    u16::from_be_bytes([data[0], data[1]])
}

/// Read a big-endian u32 from a byte slice.
pub fn read_u32_be(data: &[u8]) -> u32 {
    u32::from_be_bytes([data[0], data[1], data[2], data[3]])
}

/// Read a big-endian u64 from a byte slice.
pub fn read_u64_be(data: &[u8]) -> u64 {
    u64::from_be_bytes([
        data[0], data[1], data[2], data[3],
        data[4], data[5], data[6], data[7],
    ])
}
