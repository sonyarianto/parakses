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
                data[8], data[9], data[10], data[11],
                data[12], data[13], data[14], data[15],
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
