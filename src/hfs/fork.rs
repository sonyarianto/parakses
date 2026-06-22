use crate::blockio::BlockDevice;
use crate::hfs::volume_header::HfsPlusExtentDescriptor;
use std::io;

pub struct ForkReader<'a> {
    device: &'a dyn BlockDevice,
    volume_offset: u64,
    block_size: u32,
    extents: Vec<HfsPlusExtentDescriptor>,
    fork_size: u64,
}

impl<'a> ForkReader<'a> {
    pub fn new(
        device: &'a dyn BlockDevice,
        volume_offset: u64,
        block_size: u32,
        fork_size: u64,
    ) -> Self {
        Self {
            device,
            volume_offset,
            block_size,
            extents: Vec::new(),
            fork_size,
        }
    }

    pub fn set_extents(&mut self, extents: Vec<HfsPlusExtentDescriptor>) {
        self.extents = extents;
    }

    pub fn fork_size(&self) -> u64 {
        self.fork_size
    }

    pub fn read_all(&self) -> anyhow::Result<Vec<u8>> {
        if self.extents.is_empty() {
            return Ok(Vec::new());
        }
        let mut result = Vec::with_capacity(self.fork_size as usize);
        for extent in &self.extents {
            if extent.block_count == 0 {
                continue;
            }
            let data = self.read_extent(extent)?;
            result.extend_from_slice(&data);
            if result.len() >= self.fork_size as usize {
                break;
            }
        }
        result.truncate(self.fork_size as usize);
        Ok(result)
    }

    pub fn read_range(&self, offset: u64, len: u64) -> anyhow::Result<Vec<u8>> {
        if len == 0 || offset >= self.fork_size {
            return Ok(Vec::new());
        }
        let end = (offset + len).min(self.fork_size);
        let actual_len = (end - offset) as usize;
        let mut result = Vec::with_capacity(actual_len);
        let mut remaining = actual_len;
        let mut pos = offset;

        // Track cumulative fork-relative start position for each extent
        let mut fork_pos = 0u64;

        for extent in &self.extents {
            if extent.block_count == 0 {
                continue;
            }
            let extent_size = u64::from(extent.block_count) * u64::from(self.block_size);
            let extent_end = fork_pos + extent_size;

            if pos >= extent_end {
                fork_pos += extent_size;
                continue;
            }

            let ext_off = pos.saturating_sub(fork_pos);
            let ext_available = extent_size.saturating_sub(ext_off);
            let to_read = remaining.min(ext_available as usize);

            let extent_data = self.read_extent_range(extent, ext_off, to_read as u64)?;
            result.extend_from_slice(&extent_data);
            remaining -= to_read;
            pos += to_read as u64;

            fork_pos += extent_size;

            if remaining == 0 {
                break;
            }
        }

        Ok(result)
    }

    fn read_extent(&self, extent: &HfsPlusExtentDescriptor) -> io::Result<Vec<u8>> {
        let byte_offset = u64::from(extent.start_block) * u64::from(self.block_size);
        let byte_count = u64::from(extent.block_count) * u64::from(self.block_size);
        self.read_raw(byte_offset, byte_count)
    }

    fn read_extent_range(
        &self,
        extent: &HfsPlusExtentDescriptor,
        offset: u64,
        len: u64,
    ) -> io::Result<Vec<u8>> {
        let byte_offset = u64::from(extent.start_block) * u64::from(self.block_size) + offset;
        self.read_raw(byte_offset, len)
    }

    fn read_raw(&self, byte_offset: u64, byte_count: u64) -> io::Result<Vec<u8>> {
        let sector_size = u64::from(self.device.sector_size());
        let start_lba = (self.volume_offset + byte_offset) / sector_size;
        let skip = (self.volume_offset + byte_offset) % sector_size;
        let total_bytes = byte_count + skip;
        let sectors_needed = total_bytes.div_ceil(sector_size);

        if start_lba + sectors_needed > self.device.total_sectors() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Read past end of device: LBA {} + {} > {}",
                    start_lba,
                    sectors_needed,
                    self.device.total_sectors()
                ),
            ));
        }

        let mut data = Vec::with_capacity(sectors_needed as usize * sector_size as usize);
        for i in 0..sectors_needed {
            let sector = self.device.read_sector(start_lba + i)?;
            data.extend_from_slice(&sector);
        }

        let start = skip as usize;
        let end = (start + byte_count as usize).min(data.len());
        Ok(data[start..end].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockio::memfile::MemFile;

    fn make_device() -> MemFile {
        let mut data = vec![0u8; 8192]; // 16 sectors of 512
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = (i % 251) as u8; // prime modulus so each sector is unique
        }
        MemFile::new(data, 512)
    }

    fn make_reader(
        device: &dyn BlockDevice,
        extents: Vec<HfsPlusExtentDescriptor>,
        fork_size: u64,
    ) -> ForkReader<'_> {
        let mut reader = ForkReader::new(device, 0, 512, fork_size);
        reader.set_extents(extents);
        reader
    }

    #[test]
    fn test_read_all_single_extent() {
        let device = make_device();
        let extents = vec![HfsPlusExtentDescriptor {
            start_block: 0,
            block_count: 2,
        }];
        let reader = make_reader(&device, extents, 1024);
        let data = reader.read_all().unwrap();
        assert_eq!(data.len(), 1024);
        assert_eq!(data[0], 0);
        assert_eq!(data[512], 10);
    }

    #[test]
    fn test_read_all_multiple_extents() {
        let device = make_device();
        let extents = vec![
            HfsPlusExtentDescriptor {
                start_block: 0,
                block_count: 1,
            },
            HfsPlusExtentDescriptor {
                start_block: 3,
                block_count: 1,
            },
        ];
        let reader = make_reader(&device, extents, 1024);
        let data = reader.read_all().unwrap();
        assert_eq!(data.len(), 1024);
        assert_eq!(data[0], 0);
        assert_eq!(data[512], 30);
    }

    #[test]
    fn test_read_all_empty_extents() {
        let device = make_device();
        let reader = make_reader(&device, vec![], 0);
        let data = reader.read_all().unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn test_read_all_zero_block_extent() {
        let device = make_device();
        let extents = vec![HfsPlusExtentDescriptor {
            start_block: 0,
            block_count: 0,
        }];
        let reader = make_reader(&device, extents, 0);
        let data = reader.read_all().unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn test_read_range_basic() {
        let device = make_device();
        let extents = vec![HfsPlusExtentDescriptor {
            start_block: 0,
            block_count: 1,
        }];
        let reader = make_reader(&device, extents, 512);
        let data = reader.read_range(0, 4).unwrap();
        assert_eq!(data, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_read_range_entire_fork() {
        let device = make_device();
        let extents = vec![HfsPlusExtentDescriptor {
            start_block: 1,
            block_count: 2,
        }];
        let reader = make_reader(&device, extents, 1024);
        let data = reader.read_range(0, 1024).unwrap();
        assert_eq!(data.len(), 1024);
        assert_eq!(data[0], 10);
        assert_eq!(data[512], 20);
    }

    #[test]
    fn test_read_range_with_offset() {
        let device = make_device();
        let extents = vec![HfsPlusExtentDescriptor {
            start_block: 0,
            block_count: 2,
        }];
        let reader = make_reader(&device, extents, 1024);
        let data = reader.read_range(4, 4).unwrap();
        assert_eq!(data, vec![4, 5, 6, 7]);
    }

    #[test]
    fn test_read_range_cross_extent_boundary() {
        let device = make_device();
        let extents = vec![
            HfsPlusExtentDescriptor {
                start_block: 0,
                block_count: 1,
            },
            HfsPlusExtentDescriptor {
                start_block: 2,
                block_count: 1,
            },
        ];
        let reader = make_reader(&device, extents, 1024);
        let data = reader.read_range(508, 8).unwrap();
        assert_eq!(data, vec![6, 7, 8, 9, 20, 21, 22, 23]);
    }

    #[test]
    fn test_read_range_zero_length() {
        let device = make_device();
        let extents = vec![HfsPlusExtentDescriptor {
            start_block: 0,
            block_count: 1,
        }];
        let reader = make_reader(&device, extents, 512);
        let data = reader.read_range(0, 0).unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn test_read_range_past_fork() {
        let device = make_device();
        let extents = vec![HfsPlusExtentDescriptor {
            start_block: 0,
            block_count: 1,
        }];
        let reader = make_reader(&device, extents, 512);
        let data = reader.read_range(512, 100).unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn test_read_range_partial_at_end() {
        let device = make_device();
        let extents = vec![HfsPlusExtentDescriptor {
            start_block: 0,
            block_count: 1,
        }];
        let reader = make_reader(&device, extents, 512);
        let data = reader.read_range(510, 10).unwrap();
        assert_eq!(data.len(), 2);
    }

    #[test]
    fn test_fork_size() {
        let device = make_device();
        let reader = ForkReader::new(&device, 0, 512, 2048);
        assert_eq!(reader.fork_size(), 2048);
    }

    #[test]
    fn test_read_raw_with_volume_offset() {
        let mut data = vec![0u8; 2048];
        data[1024..1028].copy_from_slice(b"TEST");
        let device = MemFile::new(data, 512);

        let mut reader = ForkReader::new(&device, 1024, 512, 512);
        reader.set_extents(vec![HfsPlusExtentDescriptor {
            start_block: 0,
            block_count: 1,
        }]);
        let result = reader.read_range(0, 4).unwrap();
        assert_eq!(result, b"TEST");
    }

    #[test]
    fn test_read_past_end_of_device() {
        let device = MemFile::new(vec![0u8; 512], 512);
        let mut reader = ForkReader::new(&device, 0, 512, 1024);
        reader.set_extents(vec![HfsPlusExtentDescriptor {
            start_block: 0,
            block_count: 2,
        }]);
        let result = reader.read_all();
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_extents_read_all_truncates() {
        let device = make_device();
        let extents = vec![HfsPlusExtentDescriptor {
            start_block: 0,
            block_count: 3,
        }];
        let reader = make_reader(&device, extents, 1024);
        let data = reader.read_all().unwrap();
        assert_eq!(data.len(), 1024);
    }
}
