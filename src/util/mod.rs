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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_u16_be() {
        assert_eq!(read_u16_be(&[0x01, 0x02]), 0x0102);
        assert_eq!(read_u16_be(&[0xFF, 0xFF]), 0xFFFF);
        assert_eq!(read_u16_be(&[0x00, 0x00]), 0x0000);
        assert_eq!(read_u16_be(&[0x12, 0x34]), 0x1234);
    }

    #[test]
    fn test_read_u32_be() {
        assert_eq!(read_u32_be(&[0x00, 0x00, 0x00, 0x01]), 1);
        assert_eq!(read_u32_be(&[0x00, 0x01, 0x02, 0x03]), 0x00010203);
        assert_eq!(read_u32_be(&[0x12, 0x34, 0x56, 0x78]), 0x12345678);
        assert_eq!(read_u32_be(&[0xFF, 0xFF, 0xFF, 0xFF]), 0xFFFFFFFF);
    }

    #[test]
    fn test_read_u64_be() {
        assert_eq!(read_u64_be(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01]), 1);
        assert_eq!(read_u64_be(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]), 0x0102030405060708);
        assert_eq!(read_u64_be(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]), 0xFFFFFFFFFFFFFFFF);
    }
}
