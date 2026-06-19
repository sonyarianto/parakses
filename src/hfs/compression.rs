const CMPF_MAGIC: [u8; 4] = [0x63, 0x6D, 0x70, 0x66]; // "cmpf"
const CMPF_TYPE_ZLIB: u32 = 3;
const CMPF_TYPE_UNCOMPRESSED: u32 = 4;

/// HFS+ "cmpf" compressed file header (16 bytes, little-endian).
struct CmpfHeader {
    compression_type: u32,
    uncompressed_size: u64,
}

impl CmpfHeader {
    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 16 || data[..4] != CMPF_MAGIC {
            return None;
        }
        Some(Self {
            compression_type: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            uncompressed_size: u64::from_le_bytes([
                data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
            ]),
        })
    }
}

/// Check if `data` starts with an HFS+ "cmpf" compressed file header.
pub fn is_hfs_compressed(data: &[u8]) -> bool {
    data.len() >= 16 && data[..4] == CMPF_MAGIC
}

/// Decompress an HFS+ file that uses the "cmpf" compressed format.
/// Returns the decompressed data.
pub fn decompress_cmpf(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let header =
        CmpfHeader::parse(data).ok_or_else(|| anyhow::anyhow!("Not a cmpf compressed file"))?;

    match header.compression_type {
        CMPF_TYPE_UNCOMPRESSED => {
            let payload = &data[16..];
            let size = header.uncompressed_size as usize;
            if payload.len() >= size {
                Ok(payload[..size].to_vec())
            } else {
                let mut out = payload.to_vec();
                out.resize(size, 0);
                Ok(out)
            }
        }
        CMPF_TYPE_ZLIB => {
            #[cfg(feature = "compression")]
            {
                let payload = &data[16..];
                decompress_zlib(payload)
            }
            #[cfg(not(feature = "compression"))]
            {
                let _ = data;
                anyhow::bail!(
                    "zlib compression support not enabled (compile with --features compression)"
                )
            }
        }
        other => anyhow::bail!("Unknown cmpf compression type: {}", other),
    }
}

/// Decompress raw zlib data.
#[cfg(feature = "compression")]
fn decompress_zlib(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    let mut decoder = ZlibDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_hfs_compressed_with_magic() {
        let mut data = vec![0u8; 20];
        data[..4].copy_from_slice(b"cmpf");
        assert!(is_hfs_compressed(&data));
    }

    #[test]
    fn test_is_hfs_compressed_too_short() {
        assert!(!is_hfs_compressed(b""));
        assert!(!is_hfs_compressed(b"cmp"));
    }

    #[test]
    fn test_is_hfs_compressed_no_magic() {
        assert!(!is_hfs_compressed(
            b"CMFF\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
        ));
    }

    #[test]
    fn test_cmpf_header_parse_valid() {
        let mut data = vec![0u8; 16];
        data[..4].copy_from_slice(b"cmpf");
        data[4..8].copy_from_slice(&3u32.to_le_bytes()); // type = zlib
        data[8..16].copy_from_slice(&1024u64.to_le_bytes()); // size
        let hdr = CmpfHeader::parse(&data).unwrap();
        assert_eq!(hdr.compression_type, 3);
        assert_eq!(hdr.uncompressed_size, 1024);
    }

    #[test]
    fn test_cmpf_header_too_short() {
        assert!(CmpfHeader::parse(b"").is_none());
        assert!(CmpfHeader::parse(b"cm").is_none());
    }

    #[test]
    fn test_cmpf_header_bad_magic() {
        let data = b"XXXX\x03\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        assert!(CmpfHeader::parse(data).is_none());
    }

    #[test]
    fn test_decompress_cmpf_uncompressed() {
        let mut data = vec![0u8; 32];
        data[..4].copy_from_slice(b"cmpf");
        data[4..8].copy_from_slice(&4u32.to_le_bytes()); // type = uncompressed
        data[8..16].copy_from_slice(&13u64.to_le_bytes()); // size
        data[16..29].copy_from_slice(b"Hello, world!");
        let result = decompress_cmpf(&data).unwrap();
        assert_eq!(result, b"Hello, world!");
    }

    #[test]
    fn test_decompress_cmpf_uncompressed_padded() {
        let mut data = vec![0u8; 48];
        data[..4].copy_from_slice(b"cmpf");
        data[4..8].copy_from_slice(&4u32.to_le_bytes()); // type = uncompressed
        data[8..16].copy_from_slice(&5u64.to_le_bytes()); // size = 5
        data[16..21].copy_from_slice(b"Hello");
        let result = decompress_cmpf(&data).unwrap();
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn test_decompress_cmpf_not_cmpf() {
        let data = b"not cmpf at all!!";
        assert!(decompress_cmpf(data).is_err());
    }

    #[test]
    fn test_decompress_cmpf_unknown_type() {
        let mut data = vec![0u8; 20];
        data[..4].copy_from_slice(b"cmpf");
        data[4..8].copy_from_slice(&99u32.to_le_bytes()); // unknown type
        assert!(decompress_cmpf(&data).is_err());
    }

    #[cfg(feature = "compression")]
    #[test]
    fn test_decompress_cmpf_zlib() {
        // Create a valid zlib-compressed payload
        use flate2::Compression;
        use flate2::write::ZlibEncoder;
        use std::io::Write;

        let original = b"Hello from compressed HFS+ file!";
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut data = vec![0u8; 16 + compressed.len()];
        data[..4].copy_from_slice(b"cmpf");
        data[4..8].copy_from_slice(&3u32.to_le_bytes()); // type = zlib
        data[8..16].copy_from_slice(&(original.len() as u64).to_le_bytes());
        data[16..].copy_from_slice(&compressed);
        let result = decompress_cmpf(&data).unwrap();
        assert_eq!(result, original);
    }
}
