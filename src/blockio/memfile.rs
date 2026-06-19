use crate::blockio::BlockDevice;
use std::io;

pub struct MemFile {
    data: Vec<u8>,
    sector_size: u32,
}

impl MemFile {
    pub fn new(data: Vec<u8>, sector_size: u32) -> Self {
        Self { data, sector_size }
    }
}

impl BlockDevice for MemFile {
    fn sector_size(&self) -> u32 {
        self.sector_size
    }

    fn total_sectors(&self) -> u64 {
        self.data.len() as u64 / u64::from(self.sector_size)
    }

    fn read_sector(&self, lba: u64) -> io::Result<Vec<u8>> {
        let offset = lba as usize * self.sector_size as usize;
        if offset >= self.data.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "sector beyond end of image",
            ));
        }
        let end = (offset + self.sector_size as usize).min(self.data.len());
        let mut buf = vec![0u8; self.sector_size as usize];
        buf[..end - offset].copy_from_slice(&self.data[offset..end]);
        Ok(buf)
    }
}
