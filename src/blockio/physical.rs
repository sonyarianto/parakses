use crate::blockio::BlockDevice;
use std::io;
use std::mem::MaybeUninit;
use std::ptr;

const GENERIC_READ: u32 = 0x8000_0000;
const FILE_SHARE_READ: u32 = 1;
const FILE_SHARE_WRITE: u32 = 2;
const OPEN_EXISTING: u32 = 3;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
const FILE_BEGIN: u32 = 0;
const INVALID_HANDLE_VALUE: isize = -1;

const IOCTL_DISK_GET_DRIVE_GEOMETRY: u32 = 0x00070000;
const IOCTL_DISK_GET_LENGTH_INFO: u32 = 0x0007405C;

#[repr(C)]
struct DiskGeometry {
    cylinders: i64,
    media_type: u32,
    tracks_per_cylinder: u32,
    sectors_per_track: u32,
    bytes_per_sector: u32,
}

unsafe extern "system" {
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut std::ffi::c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: *mut std::ffi::c_void,
    ) -> isize;

    fn CloseHandle(hObject: isize) -> i32;

    fn ReadFile(
        hFile: isize,
        lpBuffer: *mut u8,
        nNumberOfBytesToRead: u32,
        lpNumberOfBytesRead: *mut u32,
        lpOverlapped: *mut std::ffi::c_void,
    ) -> i32;

    fn SetFilePointerEx(
        hFile: isize,
        liDistanceToMove: i64,
        lpNewFilePointer: *mut i64,
        dwMoveMethod: u32,
    ) -> i32;

    fn DeviceIoControl(
        hDevice: isize,
        dwIoControlCode: u32,
        lpInBuffer: *const std::ffi::c_void,
        nInBufferSize: u32,
        lpOutBuffer: *mut std::ffi::c_void,
        nOutBufferSize: u32,
        lpBytesReturned: *mut u32,
        lpOverlapped: *mut std::ffi::c_void,
    ) -> i32;
}

pub struct PhysicalDrive {
    handle: isize,
    sector_size: u32,
    total_sectors: u64,
}

impl PhysicalDrive {
    pub fn open(drive_index: u32) -> io::Result<Self> {
        let path: Vec<u16> = format!("\\\\.\\PhysicalDrive{}", drive_index)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let sector_size = Self::get_sector_size(handle)?;
        let total_sectors = Self::get_total_sectors(handle, sector_size)?;

        Ok(Self {
            handle,
            sector_size,
            total_sectors,
        })
    }

    fn get_sector_size(handle: isize) -> io::Result<u32> {
        let mut geometry = MaybeUninit::<DiskGeometry>::uninit();
        let mut returned: u32 = 0;

        let result = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_GET_DRIVE_GEOMETRY,
                ptr::null(),
                0,
                geometry.as_mut_ptr() as *mut std::ffi::c_void,
                std::mem::size_of::<DiskGeometry>() as u32,
                &mut returned,
                ptr::null_mut(),
            )
        };

        if result == 0 {
            return Err(io::Error::last_os_error());
        }

        let geometry = unsafe { geometry.assume_init() };
        Ok(geometry.bytes_per_sector)
    }

    fn get_total_sectors(handle: isize, sector_size: u32) -> io::Result<u64> {
        let mut length: i64 = 0;
        let mut returned: u32 = 0;

        let result = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_GET_LENGTH_INFO,
                ptr::null(),
                0,
                &mut length as *mut i64 as *mut std::ffi::c_void,
                std::mem::size_of::<i64>() as u32,
                &mut returned,
                ptr::null_mut(),
            )
        };

        if result == 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(length as u64 / u64::from(sector_size))
    }
}

impl BlockDevice for PhysicalDrive {
    fn sector_size(&self) -> u32 {
        self.sector_size
    }

    fn total_sectors(&self) -> u64 {
        self.total_sectors
    }

    fn read_sector(&self, lba: u64) -> io::Result<Vec<u8>> {
        let offset = lba * u64::from(self.sector_size);
        let mut buf = vec![0u8; self.sector_size as usize];
        let mut bytes_read: u32 = 0;

        let result = unsafe {
            SetFilePointerEx(
                self.handle,
                offset as i64,
                ptr::null_mut(),
                FILE_BEGIN,
            )
        };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }

        let result = unsafe {
            ReadFile(
                self.handle,
                buf.as_mut_ptr(),
                self.sector_size,
                &mut bytes_read,
                ptr::null_mut(),
            )
        };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }

        buf.truncate(bytes_read as usize);
        Ok(buf)
    }
}

impl Drop for PhysicalDrive {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}
