use crate::blockio::BlockDevice;
use std::fs::File;
use std::io;
use std::os::windows::fs::FileExt;

pub struct FileDevice {
    file: File,
    file_size: u64,
    sector_size: u32,
}

impl FileDevice {
    pub fn open(path: &std::path::Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        let file_size = metadata.len();
        Ok(Self {
            file,
            file_size,
            sector_size: 512,
        })
    }
}

impl BlockDevice for FileDevice {
    fn sector_size(&self) -> u32 {
        self.sector_size
    }

    fn total_sectors(&self) -> u64 {
        self.file_size / u64::from(self.sector_size)
    }

    fn read_sector(&self, lba: u64) -> io::Result<Vec<u8>> {
        self.read_sectors(lba, 1)
    }

    fn read_sectors(&self, start_lba: u64, count: u32) -> io::Result<Vec<u8>> {
        let size = u64::from(self.sector_size);
        let offset = start_lba * size;
        let byte_count = u64::from(count) * size;

        if offset + byte_count > self.file_size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "Read past end of image: LBA {} + {} > {}",
                    start_lba,
                    count,
                    self.total_sectors()
                ),
            ));
        }

        let mut buf = vec![0u8; byte_count as usize];
        self.file.seek_read(&mut buf, offset)?;
        Ok(buf)
    }
}
