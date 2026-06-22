# parakses

HFS+ / HFS Original volume reader for Windows 11.

## Build & Test

```
cargo build --release
cargo build --bin parakses_gui
cargo test
```

## GUI (`parakses_gui`)

Windows API (`windows` crate v0.58.0) with raw `SendMessageW` FFI fallback for common-control messages that the crate wrapper doesn't handle on Windows 11.

### Critical: LVM/CB/SB message constants

All `LVM_*W` message constants must use the **Wide** offset (not ANSI):

| Constant | Correct value |
|---|---|
| `LVM_INSERTCOLUMNW` | `LVM_FIRST + 97` (0x1061) |
| `LVM_INSERTITEMW` | `LVM_FIRST + 77` (0x104D) |
| `LVM_SETITEMTEXTW` | `LVM_FIRST + 116` (0x1074) |
| `LVM_GETITEMTEXTW` | `LVM_FIRST + 115` (0x1073) |
| `LVM_DELETEALLITEMS` | `LVM_FIRST + 9` (0x1009) |
| `LVM_GETNEXTITEM` | `LVM_FIRST + 12` (0x100C) |
| `LVM_GETCOLUMNWIDTH` | `LVM_FIRST + 75` (0x104B) |

### Raw FFI pattern

`SendMessageW` from the `windows` crate wrapper silently returns success without applying the operation for certain common-control messages on Windows 11. Use raw FFI for `CB_ADDSTRING` and `LVM_GETITEMTEXTW`:

```rust
unsafe extern "system" {
    fn SendMessageW(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize;
}
```

### Key files

- `src/bin/parakses_gui.rs` — all GUI logic, message constants, raw FFI
- `src/hfs/` — HFS+ and HFS original (0x4244) read support
- `disk_Apple_5MB.img` — test image with bare HFS original (3 root entries)
