#![windows_subsystem = "windows"]
#![allow(non_snake_case)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::mem;
use std::path::Path;
use std::ptr;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use parakses::blockio::{self, BlockDevice};
use parakses::hfs::{self, HfsVolume};
use parakses::volume::{VolumeDiscovery, windows::*};

const IDC_VOLUME_COMBO: u32 = 100;
const IDC_FILE_LIST: u32 = 101;
const IDC_PATH_TEXT: u32 = 102;
const IDC_STATUS_BAR: u32 = 103;
const IDC_GO_UP: u32 = 104;
const IDC_EXTRACT: u32 = 105;
const IDM_OPEN_IMAGE: u32 = 200;
const IDM_EXIT: u32 = 201;
const IDM_ABOUT: u32 = 300;

const LVM_FIRST: u32 = 0x1000;
const LVM_INSERTCOLUMNW: u32 = LVM_FIRST + 1;
const LVM_DELETEALLITEMS: u32 = LVM_FIRST + 9;
const LVM_INSERTITEMW: u32 = LVM_FIRST + 7;
const LVM_SETITEMTEXTW: u32 = LVM_FIRST + 52;
const LVM_GETNEXTITEM: u32 = LVM_FIRST + 12;
const LVM_GETITEMTEXTW: u32 = LVM_FIRST + 45;
const LVM_SETEXTENDEDLISTVIEWSTYLE: u32 = LVM_FIRST + 54;
const LVNI_SELECTED: u32 = 0x0002;
const LVS_EX_FULLROWSELECT: u32 = 0x00000020;
const LVS_EX_GRIDLINES: u32 = 0x00000001;
const LVIF_TEXT: u32 = 0x0001;
const LVCF_FMT: u32 = 0x0001;
const LVCF_WIDTH: u32 = 0x0002;
const LVCF_TEXT: u32 = 0x0004;
const LVCFMT_LEFT: i32 = 0;
const LVCFMT_RIGHT: i32 = 1;
const CB_ADDSTRING: u32 = 0x0143;
const CB_SETCURSEL: u32 = 0x014E;
const CB_GETCURSEL: u32 = 0x0148;
const CB_SETITEMDATA: u32 = 0x014B;
const CB_GETITEMDATA: u32 = 0x014A;
const CB_RESETCONTENT: u32 = 0x014F;
const CB_ERR: isize = -1;
const BN_CLICKED: u32 = 0;
const CBN_SELCHANGE: u32 = 1;
const SB_SETTEXT: u32 = 0x0400 + 1;
const LVS_REPORT: u32 = 0x0001;
const LVS_SINGLESEL: u32 = 0x0004;
const LVS_SHOWSELALWAYS: u32 = 0x0008;
const WS_CHILD: u32 = 0x40000000;
const WS_VISIBLE: u32 = 0x10000000;
const WS_TABSTOP: u32 = 0x00010000;
const ES_READONLY: u32 = 0x00000800;
const BS_PUSHBUTTON: u32 = 0x00000000;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const SBARS_SIZEGRIP: u32 = 0x0100;
const WS_EX_CLIENTEDGE: u32 = 0x00000200;
const WS_EX_WINDOWEDGE: u32 = 0x00000100;

#[repr(C)]
struct LVITEMW {
    mask: u32,
    iItem: i32,
    iSubItem: i32,
    state: u32,
    stateMask: u32,
    pszText: *mut u16,
    cchTextMax: i32,
    iImage: i32,
    lParam: isize,
    iIndent: i32,
}

#[repr(C)]
struct LVCOLUMNW {
    mask: u32,
    fmt: i32,
    cx: i32,
    pszText: *const u16,
    cchTextMax: i32,
    iSubItem: i32,
    iImage: i32,
    iOrder: i32,
}

#[repr(C)]
struct OPENFILENAMEW {
    lStructSize: u32,
    hwndOwner: HWND,
    hInstance: HINSTANCE,
    lpstrFilter: *mut u16,
    lpstrCustomFilter: *mut u16,
    nMaxCustFilter: u32,
    nFilterIndex: u32,
    lpstrFile: *mut u16,
    nMaxFile: u32,
    lpstrFileTitle: *mut u16,
    nMaxFileTitle: u32,
    lpstrInitialDir: *mut u16,
    lpstrTitle: *mut u16,
    Flags: u32,
    nFileOffset: u16,
    nFileExtension: u16,
    lpstrDefExt: *mut u16,
    lCustData: isize,
    lpfnHook: Option<unsafe extern "system" fn(HWND, u32, usize, isize) -> usize>,
    lpTemplateName: *mut u16,
    pvReserved: *mut std::ffi::c_void,
    dwReserved: u32,
    FlagsEx: u32,
}

#[repr(C)]
struct NMHDR {
    hwndFrom: HWND,
    idFrom: usize,
    code: u32,
}

#[link(name = "comdlg32")]
unsafe extern "system" {
    fn GetOpenFileNameW(ofn: *mut OPENFILENAMEW) -> BOOL;
    fn GetSaveFileNameW(ofn: *mut OPENFILENAMEW) -> BOOL;
}

struct VolumeEntry {
    display: String,
    drive_index: u32,
    partition_index: usize,
    info: WindowsVolume,
}

struct GuiState {
    volumes: Vec<VolumeEntry>,
    current_volume_idx: Option<usize>,
    current_path: String,
    image_path: Option<String>,
    hwnd: HWND,
    hwnd_combo: HWND,
    hwnd_list: HWND,
    hwnd_path: HWND,
    hwnd_up: HWND,
    hwnd_extract: HWND,
    hwnd_status: HWND,
}

static mut APP_HINSTANCE: Option<HINSTANCE> = None;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide_lossy(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{} KB", bytes / 1024)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{} MB", bytes / (1024 * 1024))
    } else {
        format!("{} GB", bytes / (1024 * 1024 * 1024))
    }
}

fn show_error(hwnd: HWND, msg: &str) {
    let w = to_wide(msg);
    unsafe {
        let _ = MessageBoxW(hwnd, PCWSTR(w.as_ptr()), w!("Error"), MB_OK);
    }
}

fn show_about(hwnd: HWND) {
    let msg = to_wide("parakses v0.1.0\nHFS+ Reader for Windows 11");
    unsafe {
        let _ = MessageBoxW(hwnd, PCWSTR(msg.as_ptr()), w!("About parakses"), MB_OK);
    }
}

fn open_hfs(state: &GuiState, vol_idx: usize) -> Result<HfsVolume, String> {
    let entry = &state.volumes[vol_idx];
    let part = &entry.info.hfs_partitions[entry.partition_index];

    if let Some(img) = &state.image_path {
        let path = Path::new(img);
        let drive = blockio::filedevice::FileDevice::open(path)
            .map_err(|e| format!("Failed to open image: {}", e))?;
        let sector_size = drive.sector_size();
        let volume_offset = part.start_lba * u64::from(sector_size);
        HfsVolume::open(Box::new(drive), volume_offset)
            .map_err(|e| format!("Failed to open HFS+ volume: {}", e))
    } else {
        let drive = blockio::physical::PhysicalDrive::open(entry.drive_index)
            .map_err(|e| format!("Failed to open drive: {}", e))?;
        let sector_size = drive.sector_size();
        let volume_offset = part.start_lba * u64::from(sector_size);
        HfsVolume::open(Box::new(drive), volume_offset)
            .map_err(|e| format!("Failed to open HFS+ volume: {}", e))
    }
}

fn populate_list(state: &GuiState) {
    let hwnd_list = state.hwnd_list;
    unsafe {
        let _ = SendMessageW(hwnd_list, LVM_DELETEALLITEMS, WPARAM(0), LPARAM(0));
    }

    let vol_idx = match state.current_volume_idx {
        Some(i) => i,
        None => return,
    };

    let hfs = match open_hfs(state, vol_idx) {
        Ok(h) => h,
        Err(_) => return,
    };

    let entries = if state.current_path == "/" {
        hfs.list_root()
    } else {
        let record = match hfs.resolve_path(&state.current_path) {
            Ok(r) => r,
            Err(_) => return,
        };
        match record {
            hfs::catalog::CatalogRecordData::Folder(f) => hfs.list_directory(f.folder_id),
            _ => return,
        }
    };

    let entries = match entries {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut row = 0i32;

    if state.current_path != "/" {
        unsafe {
            let mut item: LVITEMW = mem::zeroed();
            item.mask = LVIF_TEXT;
            item.iItem = row;
            let mut w = to_wide("..");
            item.pszText = w.as_mut_ptr();
            let _ = SendMessageW(
                hwnd_list,
                LVM_INSERTITEMW,
                WPARAM(0),
                LPARAM(&item as *const _ as isize),
            );
            item.iSubItem = 2;
            let mut w_type = to_wide("Directory");
            item.pszText = w_type.as_mut_ptr();
            let _ = SendMessageW(
                hwnd_list,
                LVM_SETITEMTEXTW,
                WPARAM(row as usize),
                LPARAM(&item as *const _ as isize),
            );
        }
        row += 1;
    }

    for entry in &entries {
        unsafe {
            let mut item: LVITEMW = mem::zeroed();
            item.mask = LVIF_TEXT;
            item.iItem = row;
            item.iSubItem = 0;
            let mut w = to_wide(&entry.name);
            item.pszText = w.as_mut_ptr();
            let _ = SendMessageW(
                hwnd_list,
                LVM_INSERTITEMW,
                WPARAM(0),
                LPARAM(&item as *const _ as isize),
            );

            item.iSubItem = 1;
            let size_str = if entry.is_directory {
                String::new()
            } else {
                format_size(entry.size)
            };
            let mut w_size = to_wide(&size_str);
            item.pszText = w_size.as_mut_ptr();
            let _ = SendMessageW(
                hwnd_list,
                LVM_SETITEMTEXTW,
                WPARAM(row as usize),
                LPARAM(&item as *const _ as isize),
            );

            item.iSubItem = 2;
            let kind = if entry.is_directory {
                "Directory"
            } else {
                "File"
            };
            let mut w_kind = to_wide(kind);
            item.pszText = w_kind.as_mut_ptr();
            let _ = SendMessageW(
                hwnd_list,
                LVM_SETITEMTEXTW,
                WPARAM(row as usize),
                LPARAM(&item as *const _ as isize),
            );
        }
        row += 1;
    }
}

fn refresh_list(state: &GuiState) {
    let path_w = to_wide(&state.current_path);
    unsafe {
        let _ = SetWindowTextW(state.hwnd_path, PCWSTR(path_w.as_ptr()));
    }

    populate_list(state);

    let vol_idx = match state.current_volume_idx {
        Some(i) => i,
        None => {
            unsafe {
                let _ = SetWindowTextW(state.hwnd_status, w!("No volume selected"));
            }
            return;
        }
    };

    match open_hfs(state, vol_idx) {
        Ok(hfs) => {
            let info = hfs.volume_info();
            let status = format!(
                "Volume: {} | Files: {} | Folders: {} | Free: {}",
                info.volume_name,
                info.file_count,
                info.folder_count,
                format_size(u64::from(info.free_blocks) * u64::from(info.block_size))
            );
            let w = to_wide(&status);
            unsafe {
                let _ = SetWindowTextW(state.hwnd_status, PCWSTR(w.as_ptr()));
            }
        }
        Err(e) => {
            let w = to_wide(&e);
            unsafe {
                let _ = SetWindowTextW(state.hwnd_status, PCWSTR(w.as_ptr()));
            }
        }
    }
}

fn load_volumes(state: &mut GuiState) {
    state.volumes.clear();
    let mut items: Vec<VolumeEntry> = Vec::new();

    if let Ok(vols) = WindowsVolumeEnumerator::enumerate() {
        for v in &vols {
            for (pi, _part) in v.hfs_partitions.iter().enumerate() {
                let vol_name = format!("PhysicalDrive{}", v.drive_index);
                let display = if v.hfs_partitions.len() > 1 {
                    format!("{} (partition {})", vol_name, pi)
                } else {
                    vol_name
                };
                items.push(VolumeEntry {
                    display,
                    drive_index: v.drive_index,
                    partition_index: pi,
                    info: WindowsVolume {
                        drive_index: v.drive_index,
                        hfs_partitions: v.hfs_partitions.clone(),
                    },
                });
            }
        }
    }

    if let Some(img) = &state.image_path {
        let path = Path::new(img);
        if path.exists() {
            if let Ok(drive) = blockio::filedevice::FileDevice::open(path) {
                if let Ok(vols) = WindowsVolumeEnumerator::enumerate_from(&drive) {
                    for v in &vols {
                        for (pi, _part) in v.hfs_partitions.iter().enumerate() {
                            let display = format!("Image: {} (partition {})", img, pi);
                            items.push(VolumeEntry {
                                display,
                                drive_index: v.drive_index,
                                partition_index: pi,
                                info: WindowsVolume {
                                    drive_index: v.drive_index,
                                    hfs_partitions: v.hfs_partitions.clone(),
                                },
                            });
                        }
                    }
                }
            }
        }
    }

    unsafe {
        let combo = state.hwnd_combo;
        let _ = SendMessageW(combo, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
        for (i, entry) in items.iter().enumerate() {
            let w = to_wide(&entry.display);
            let idx = SendMessageW(combo, CB_ADDSTRING, WPARAM(0), LPARAM(w.as_ptr() as isize));
            let _ = SendMessageW(
                combo,
                CB_SETITEMDATA,
                WPARAM(idx.0 as usize),
                LPARAM(i as isize),
            );
        }
    }

    state.volumes = items;
    if !state.volumes.is_empty() {
        unsafe {
            let _ = SendMessageW(state.hwnd_combo, CB_SETCURSEL, WPARAM(0), LPARAM(0));
        }
        state.current_volume_idx = Some(0);
        state.current_path = "/".to_string();
        refresh_list(state);
    }
}

fn on_volume_selected(state: &mut GuiState) {
    unsafe {
        let combo = state.hwnd_combo;
        let sel = SendMessageW(combo, CB_GETCURSEL, WPARAM(0), LPARAM(0));
        if sel.0 == CB_ERR {
            return;
        }
        let data = SendMessageW(combo, CB_GETITEMDATA, WPARAM(sel.0 as usize), LPARAM(0));
        state.current_volume_idx = Some(data.0 as usize);
        state.current_path = "/".to_string();
        refresh_list(state);
    }
}

fn on_go_up(state: &mut GuiState) {
    if state.current_path == "/" {
        return;
    }
    let trimmed = state.current_path.trim_end_matches('/');
    let parent = match trimmed.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => trimmed[..pos].to_string(),
        None => "/".to_string(),
    };
    state.current_path = parent;
    refresh_list(state);
}

fn get_selected_item_name(hwnd_list: HWND) -> Option<String> {
    unsafe {
        let sel = SendMessageW(
            hwnd_list,
            LVM_GETNEXTITEM,
            WPARAM((-1i32) as usize),
            LPARAM(LVNI_SELECTED as isize),
        );
        if sel.0 == -1 {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut item: LVITEMW = mem::zeroed();
        item.iSubItem = 0;
        item.pszText = buf.as_mut_ptr();
        item.cchTextMax = buf.len() as i32;
        item.mask = LVIF_TEXT;
        let _ = SendMessageW(
            hwnd_list,
            LVM_GETITEMTEXTW,
            WPARAM(sel.0 as usize),
            LPARAM(&item as *const _ as isize),
        );
        let name = from_wide_lossy(&buf);
        Some(name)
    }
}

fn on_list_double_click(state: &mut GuiState) {
    let hwnd_list = state.hwnd_list;
    let name = match get_selected_item_name(hwnd_list) {
        Some(n) => n,
        None => return,
    };
    if name == ".." {
        on_go_up(state);
        return;
    }

    let vol_idx = match state.current_volume_idx {
        Some(i) => i,
        None => return,
    };
    let hfs = match open_hfs(state, vol_idx) {
        Ok(h) => h,
        Err(_) => return,
    };

    let entries = if state.current_path == "/" {
        hfs.list_root()
    } else {
        let record = match hfs.resolve_path(&state.current_path) {
            Ok(r) => r,
            Err(_) => return,
        };
        match record {
            hfs::catalog::CatalogRecordData::Folder(f) => hfs.list_directory(f.folder_id),
            _ => return,
        }
    };

    let entries = match entries {
        Ok(e) => e,
        Err(_) => return,
    };

    let offset = if state.current_path != "/" { 1 } else { 0 };
    unsafe {
        let sel = SendMessageW(
            hwnd_list,
            LVM_GETNEXTITEM,
            WPARAM((-1i32) as usize),
            LPARAM(LVNI_SELECTED as isize),
        );
        let idx = (sel.0 - offset as isize) as usize;
        if idx < entries.len() && entries[idx].is_directory {
            state.current_path = if state.current_path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", state.current_path, name)
            };
            refresh_list(state);
        }
    }
}

fn on_extract(state: &mut GuiState) {
    let hwnd_list = state.hwnd_list;
    let name = match get_selected_item_name(hwnd_list) {
        Some(n) => n,
        None => {
            show_error(state.hwnd, "No file selected.");
            return;
        }
    };
    if name == ".." {
        return;
    }

    let vol_idx = match state.current_volume_idx {
        Some(i) => i,
        None => return,
    };
    let hfs = match open_hfs(state, vol_idx) {
        Ok(h) => h,
        Err(e) => {
            show_error(state.hwnd, &e);
            return;
        }
    };

    let entries = if state.current_path == "/" {
        hfs.list_root()
    } else {
        let record = match hfs.resolve_path(&state.current_path) {
            Ok(r) => r,
            Err(e) => {
                show_error(state.hwnd, &format!("{}", e));
                return;
            }
        };
        match record {
            hfs::catalog::CatalogRecordData::Folder(f) => hfs.list_directory(f.folder_id),
            _ => return,
        }
    };

    let entries = match entries {
        Ok(e) => e,
        Err(e) => {
            show_error(state.hwnd, &format!("{}", e));
            return;
        }
    };

    let offset = if state.current_path != "/" { 1 } else { 0 };
    unsafe {
        let sel = SendMessageW(
            hwnd_list,
            LVM_GETNEXTITEM,
            WPARAM((-1i32) as usize),
            LPARAM(LVNI_SELECTED as isize),
        );
        let idx = (sel.0 - offset as isize) as usize;
        if idx >= entries.len() {
            return;
        }
        if entries[idx].is_directory {
            state.current_path = if state.current_path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", state.current_path, name)
            };
            refresh_list(state);
            return;
        }
    }

    let file_path = if state.current_path == "/" {
        format!("/{}", name)
    } else {
        format!("{}/{}", state.current_path, name)
    };

    let mut out_buf = vec![0u16; 4096];
    let name_w = to_wide(&name);
    let copy_len = name_w.len().min(4095);
    out_buf[..copy_len].copy_from_slice(&name_w[..copy_len]);

    let mut filter = to_wide("All Files\0*.*\0");
    let mut ofn = OPENFILENAMEW {
        lStructSize: mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: state.hwnd,
        hInstance: HINSTANCE::default(),
        lpstrFilter: filter.as_mut_ptr(),
        lpstrCustomFilter: ptr::null_mut(),
        nMaxCustFilter: 0,
        nFilterIndex: 1,
        lpstrFile: out_buf.as_mut_ptr(),
        nMaxFile: out_buf.len() as u32,
        lpstrFileTitle: ptr::null_mut(),
        nMaxFileTitle: 0,
        lpstrInitialDir: ptr::null_mut(),
        lpstrTitle: ptr::null_mut(),
        Flags: 0x0002 | 0x0004,
        nFileOffset: 0,
        nFileExtension: 0,
        lpstrDefExt: ptr::null_mut(),
        lCustData: 0,
        lpfnHook: None,
        lpTemplateName: ptr::null_mut(),
        pvReserved: ptr::null_mut(),
        dwReserved: 0,
        FlagsEx: 0,
    };

    let result = unsafe { GetSaveFileNameW(&mut ofn) };
    if result == BOOL(0) {
        return;
    }

    let dst = from_wide_lossy(&out_buf);
    let dst_path = Path::new(&dst);
    match hfs.extract_file(&file_path, dst_path) {
        Ok(size) => {
            let msg = format!("Extracted {} bytes to '{}'", size, dst);
            let w = to_wide(&msg);
            unsafe {
                let _ = MessageBoxW(state.hwnd, PCWSTR(w.as_ptr()), w!("Extract"), MB_OK);
            }
        }
        Err(e) => {
            show_error(state.hwnd, &format!("Extract failed: {}", e));
        }
    }
}

fn on_open_image(state: &mut GuiState) {
    let mut buf = vec![0u16; 4096];
    let mut filter = to_wide("Disk Images\0*.img;*.dmg;*.raw;*.dd\0All Files\0*.*\0");
    let mut ofn = OPENFILENAMEW {
        lStructSize: mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: state.hwnd,
        hInstance: HINSTANCE::default(),
        lpstrFilter: filter.as_mut_ptr(),
        lpstrCustomFilter: ptr::null_mut(),
        nMaxCustFilter: 0,
        nFilterIndex: 1,
        lpstrFile: buf.as_mut_ptr(),
        nMaxFile: buf.len() as u32,
        lpstrFileTitle: ptr::null_mut(),
        nMaxFileTitle: 0,
        lpstrInitialDir: ptr::null_mut(),
        lpstrTitle: ptr::null_mut(),
        Flags: 0x00001000 | 0x00000004,
        nFileOffset: 0,
        nFileExtension: 0,
        lpstrDefExt: ptr::null_mut(),
        lCustData: 0,
        lpfnHook: None,
        lpTemplateName: ptr::null_mut(),
        pvReserved: ptr::null_mut(),
        dwReserved: 0,
        FlagsEx: 0,
    };

    let result = unsafe { GetOpenFileNameW(&mut ofn) };
    if result == BOOL(0) {
        return;
    }

    let path = from_wide_lossy(&buf);
    state.image_path = Some(path);
    state.current_path = "/".to_string();
    load_volumes(state);
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CREATE {
        let mut state = create_gui(hwnd);
        load_volumes(&mut state);
        let state_ptr = Box::into_raw(Box::new(state));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        return LRESULT(0);
    }

    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    if state_ptr == 0 {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &mut *(state_ptr as *mut GuiState);

    match msg {
        WM_DESTROY => {
            let _ = Box::from_raw(state_ptr as *mut GuiState);
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_SIZE => {
            let w = (lparam.0 & 0xFFFF) as i32;
            let h = ((lparam.0 >> 16) & 0xFFFF) as i32;
            let pad = 4i32;
            let cy = 26i32;
            let combo_w = 200i32;
            let btn_up_w = 50i32;
            let btn_extract_w = 70i32;
            let btn_gap = 4i32;
            let _ = SetWindowPos(
                state.hwnd_combo,
                HWND(ptr::null_mut()),
                pad,
                pad,
                combo_w,
                200,
                SET_WINDOW_POS_FLAGS(0x0004),
            );
            let path_x = pad + combo_w + pad;
            let buttons_w = btn_up_w + btn_gap + btn_extract_w + pad;
            let path_w = (w - path_x - pad - buttons_w).max(50);
            let btn_x = path_x + path_w + pad;
            let _ = SetWindowPos(
                state.hwnd_path,
                HWND(ptr::null_mut()),
                path_x,
                pad,
                path_w,
                cy,
                SET_WINDOW_POS_FLAGS(0x0004),
            );
            let _ = SetWindowPos(
                state.hwnd_up,
                HWND(ptr::null_mut()),
                btn_x,
                pad,
                btn_up_w,
                cy,
                SET_WINDOW_POS_FLAGS(0x0004),
            );
            let _ = SetWindowPos(
                state.hwnd_extract,
                HWND(ptr::null_mut()),
                btn_x + btn_up_w + btn_gap,
                pad,
                btn_extract_w,
                cy,
                SET_WINDOW_POS_FLAGS(0x0004),
            );
            let list_top = cy + pad * 2;
            let _ = SetWindowPos(
                state.hwnd_list,
                HWND(ptr::null_mut()),
                pad,
                list_top,
                w - pad * 2,
                h - list_top - pad - 20,
                SET_WINDOW_POS_FLAGS(0x0004),
            );
            let _ = SendMessageW(state.hwnd_status, SB_SETTEXT, WPARAM(0), LPARAM(0));
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as u32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            match id {
                IDC_GO_UP if code == BN_CLICKED => on_go_up(state),
                IDC_EXTRACT if code == BN_CLICKED => on_extract(state),
                IDC_VOLUME_COMBO if code == CBN_SELCHANGE => on_volume_selected(state),
                IDM_OPEN_IMAGE => on_open_image(state),
                IDM_EXIT => {
                    let _ = DestroyWindow(hwnd);
                }
                IDM_ABOUT => show_about(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_NOTIFY => {
            let nmhdr = &*(lparam.0 as *const NMHDR);
            if nmhdr.hwndFrom == state.hwnd_list && nmhdr.code == (NM_DBLCLK as u32) {
                on_list_double_click(state);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn create_gui(hwnd: HWND) -> GuiState {
    let hinst = unsafe { APP_HINSTANCE.unwrap() };
    unsafe {
        let hwnd_combo = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_CLIENTEDGE),
            w!("ComboBox"),
            w!(""),
            WINDOW_STYLE(CBS_DROPDOWNLIST | WS_CHILD | WS_VISIBLE | WS_TABSTOP),
            10,
            4,
            280,
            200,
            hwnd,
            HMENU(IDC_VOLUME_COMBO as *mut _),
            hinst,
            None,
        )
        .unwrap();

        let hwnd_path = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_CLIENTEDGE),
            w!("Edit"),
            w!("/"),
            WINDOW_STYLE(WS_CHILD | WS_VISIBLE | ES_READONLY),
            300,
            4,
            200,
            26,
            hwnd,
            HMENU(IDC_PATH_TEXT as *mut _),
            hinst,
            None,
        )
        .unwrap();

        let hwnd_up = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_WINDOWEDGE),
            w!("Button"),
            w!("Up"),
            WINDOW_STYLE(WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON | WS_TABSTOP),
            510,
            4,
            50,
            26,
            hwnd,
            HMENU(IDC_GO_UP as *mut _),
            hinst,
            None,
        )
        .unwrap();

        let hwnd_extract = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_WINDOWEDGE),
            w!("Button"),
            w!("Extract"),
            WINDOW_STYLE(WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON | WS_TABSTOP),
            570,
            4,
            70,
            26,
            hwnd,
            HMENU(IDC_EXTRACT as *mut _),
            hinst,
            None,
        )
        .unwrap();

        let hwnd_list = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_CLIENTEDGE),
            w!("SysListView32"),
            w!(""),
            WINDOW_STYLE(
                WS_CHILD | WS_VISIBLE | LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS | WS_TABSTOP,
            ),
            10,
            34,
            780,
            450,
            hwnd,
            HMENU(IDC_FILE_LIST as *mut _),
            hinst,
            None,
        )
        .unwrap();

        let _ = SendMessageW(
            hwnd_list,
            LVM_SETEXTENDEDLISTVIEWSTYLE,
            WPARAM(0),
            LPARAM((LVS_EX_FULLROWSELECT | LVS_EX_GRIDLINES) as isize),
        );

        let headers = ["Name", "Size", "Type"];
        let widths = [360, 100, 100];
        for (i, (&h, &w)) in headers.iter().zip(widths.iter()).enumerate() {
            let h_w = to_wide(h);
            let lvc = LVCOLUMNW {
                mask: LVCF_FMT | LVCF_WIDTH | LVCF_TEXT,
                fmt: if i == 1 { LVCFMT_RIGHT } else { LVCFMT_LEFT },
                cx: w,
                pszText: h_w.as_ptr(),
                cchTextMax: h.len() as i32,
                iSubItem: i as i32,
                iImage: 0,
                iOrder: 0,
            };
            let _ = SendMessageW(
                hwnd_list,
                LVM_INSERTCOLUMNW,
                WPARAM(i),
                LPARAM(&lvc as *const _ as isize),
            );
        }

        let hwnd_status = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_CLIENTEDGE),
            w!("msctls_statusbar32"),
            w!("Ready"),
            WINDOW_STYLE(WS_CHILD | WS_VISIBLE | SBARS_SIZEGRIP),
            0,
            0,
            0,
            0,
            hwnd,
            HMENU(IDC_STATUS_BAR as *mut _),
            hinst,
            None,
        )
        .unwrap();

        let hmenu = CreateMenu().unwrap();
        let hsub = CreatePopupMenu().unwrap();
        let _ = AppendMenuW(
            hsub,
            MENU_ITEM_FLAGS(0),
            IDM_OPEN_IMAGE as usize,
            w!("Open Image..."),
        );
        let _ = AppendMenuW(hsub, MENU_ITEM_FLAGS(0x0800), 0, PCWSTR(ptr::null()));
        let _ = AppendMenuW(hsub, MENU_ITEM_FLAGS(0), IDM_EXIT as usize, w!("Exit"));
        let _ = AppendMenuW(
            hmenu,
            MENU_ITEM_FLAGS(0x0010),
            hsub.0 as u32 as usize,
            w!("File"),
        );

        let habout = CreatePopupMenu().unwrap();
        let _ = AppendMenuW(
            habout,
            MENU_ITEM_FLAGS(0),
            IDM_ABOUT as usize,
            w!("About parakses"),
        );
        let _ = AppendMenuW(
            hmenu,
            MENU_ITEM_FLAGS(0x0010),
            habout.0 as u32 as usize,
            w!("Help"),
        );

        let _ = SetMenu(hwnd, hmenu);

        GuiState {
            volumes: Vec::new(),
            current_volume_idx: None,
            current_path: "/".to_string(),
            image_path: None,
            hwnd,
            hwnd_combo,
            hwnd_list,
            hwnd_path,
            hwnd_up,
            hwnd_extract,
            hwnd_status,
        }
    }
}

fn main() {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        APP_HINSTANCE = Some(hinst);

        let mut wc: WNDCLASSW = mem::zeroed();
        wc.style = CS_HREDRAW | CS_VREDRAW;
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = hinst;
        wc.hIcon = LoadIconW(hinst, IDI_APPLICATION).unwrap_or(HICON::default());
        wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap_or(HCURSOR::default());
        wc.hbrBackground = HBRUSH((COLOR_BTNFACE.0 + 1) as *mut std::ffi::c_void);
        wc.lpszClassName = w!("parakses_gui");

        if RegisterClassW(&wc) == 0 {
            return;
        }

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_WINDOWEDGE),
            w!("parakses_gui"),
            w!("parakses - HFS+ Browser"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            800,
            550,
            None,
            None,
            hinst,
            None,
        );

        let hwnd = match hwnd {
            Ok(h) => h,
            Err(_) => return,
        };

        let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
        let _ = UpdateWindow(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
}
