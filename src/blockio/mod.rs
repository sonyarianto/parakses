pub mod filedevice;
pub mod memfile;
pub mod physical;

use std::io;

pub trait BlockDevice {
    fn sector_size(&self) -> u32;
    fn total_sectors(&self) -> u64;
    fn read_sector(&self, lba: u64) -> io::Result<Vec<u8>>;
    fn read_sectors(&self, start_lba: u64, count: u32) -> io::Result<Vec<u8>> {
        let mut data = Vec::with_capacity(count as usize * self.sector_size() as usize);
        for i in 0..count {
            let sector = self.read_sector(start_lba + u64::from(i))?;
            data.extend_from_slice(&sector);
        }
        Ok(data)
    }
}
