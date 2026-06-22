# HFS+ Reader for Windows 11 — Implementation Status

## Goal

A native Rust application that can read **HFS+ (Mac OS Extended)** volumes on Windows 11, with both a **CLI** and a **native Windows GUI**. The user connects an HFS+-formatted disk (flash / external) or loads a disk image and can browse its contents and extract files.

## Architecture

```
┌──────────────────────────────────────────┐
│        parakses (CLI) / parakses_gui     │
│    volumes | list | cat | extract        │
│    or native Win32 GUI window            │
├──────────────────────────────────────────┤
│              Library (lib.rs)            │
│    Shared HFS+ logic used by both        │
├──────────────────────────────────────────┤
│          HFS+ Parser (pure Rust)        │
│  ┌──────────┐ ┌──────────┐ ┌─────────┐  │
│  │ Volume   │ │ Catalog  │ │ Extents │  │
│  │ Header   │ │ B-tree   │ │ B-tree  │  │
│  └──────────┘ └──────────┘ └─────────┘  │
│  ┌──────────┐ ┌──────────┐ ┌─────────┐  │
│  │ HFS+     │ │ Fork     │ │ Comp-   │  │
│  │ Wrapper  │ │ Reader   │ │ ression │  │
│  └──────────┘ └──────────┘ └─────────┘  │
├──────────────────────────────────────────┤
│      Windows Raw Disk Layer              │
│  (Win32 FFI: CreateFile on               │
│   \\.\PhysicalDriveN )                   │
├──────────────────────────────────────────┤
│      Windows Volume Discovery            │
│  (MBR + GPT partition table parsing,     │
│   detection via type 0xAF / Apple GUID)  │
└──────────────────────────────────────────┘
```

## Implementation Status

### Phase 0 — Project Scaffolding ✅

- Cargo project initialized (`cargo init --name parakses`)
- Dependencies: `clap`, `anyhow`, `thiserror`, `log`, `env_logger`, `uuid`, `flate2` (optional)
- Full module skeleton with all directories and stub files

### Phase 1 — Windows Volume Discovery ✅

- Enumerates `\\.\PhysicalDrive0` through `\\.\PhysicalDrive31`
- Opens each drive with `CreateFileW` (raw Win32 FFI, no windows-sys crate)
- Reads MBR (sector 0) to detect partition tables
- Parses MBR partition entries (offset 0x1BE, type 0xAF = Apple HFS+)
- Parses GPT header (LBA 1) + partition entries (Apple HFS+ GUID: `48465300-...`)
- Returns list of detected HFS+ volumes with partition info

### Phase 2 — Raw Block I/O ✅

- `BlockDevice` trait with `sector_size()`, `total_sectors()`, `read_sector()`, `read_sectors()`
- `PhysicalDrive` struct: opens with `CreateFileW`, gets sector size via `IOCTL_DISK_GET_DRIVE_GEOMETRY`, reads sectors with `SetFilePointerEx` + `ReadFile`
- `MemFile` struct: in-memory block device for testing
- Sector size detection (typically 512 or 4096)

### Phase 3 — HFS+ Volume Header ✅

- Full 512-byte Volume Header parser at offset 1024 from partition start
- Validates signature (`H+` = 0x482B, `HX` = 0x4858)
- Parses: block size, total/free blocks, file/folder counts, attributes, dates
- `HfsPlusExtentDescriptor` for allocation block extent runs
- Extent descriptors for 5 special files: allocation, extents overflow, catalog, attributes, startup

### Phase 4 — B-tree Engine ✅

- `BTreeReader`: reads HFS+ B-tree files via `ForkReader`
- Header node parsing: tree depth, root node, leaf chain (first/last leaf), node size
- `NodeDescriptor`: forward/backward links, node type (header/index/leaf/map), height
- Record offset table parsing (offsets stored at end of node)
- Leaf node iteration via forward-linked list
- Index node traversal for key-based search
- Generic enough for Catalog, ExtentsOverflow, and Attributes trees

### Phase 5 — Catalog B-tree & Directory Traversal ✅

- `CatalogReader`: walks Catalog B-tree leaves, filters by parent CNID
- Catalog key parsing: `HfsPlusCatalogKeyRaw` (parentID + UTF-16BE name)
- Catalog record parsing:
  - `kHFSPlusFolderRecord` (0x0001) — folder metadata (folderID, valence, dates)
  - `kHFSPlusFileRecord` (0x0002) — file metadata + data/resource fork info
  - `kHFSPlusFolderThreadRecord` / `kHFSPlusFileThreadRecord` (0x0003/0x0004) — name resolution
- `HfsVolume::resolve_path()` traverses `/dir/subdir/file` via catalog lookup
- `HfsVolume::list_directory(parent_id)` returns sorted `DirEntry` list

### Phase 6 — Extents Overflow & Data Fork Reading ✅

- `HfsPlusForkData` struct: logical size, clump size, total blocks, 8 inline extents
- `ExtentsOverflowReader`: looks up additional extents in the ExtentsOverflow B-tree when a file has more than 8 extent runs
- `HfsVolume::build_extents()`: combines inline + overflow extents into complete list
- `ForkReader::read_all()` / `read_range()`: maps allocation blocks → sector reads
- `HfsVolume::read_file_data()`: reads a file's data fork content

### Phase 7 — File Extraction ✅

- `parakses extract <volume> <src> <dst>` — reads file from HFS+ volume, writes to Windows filesystem
- `parakses cat <volume> <path>` — dumps file content to stdout (binary-safe)
- Path resolution supports arbitrary depth: `/dir1/dir2/dir3/file.txt`

### Phase 8 — Compression Support ✅

- HFS+ "cmpf" format detection and decompression
- 16-byte header: magic (`cmpf`), compression type (3=zlib, 4=uncompressed), uncompressed size
- `is_hfs_compressed()` / `decompress_cmpf()` in `compression.rs`
- Zlib decompression via `flate2` crate (gated behind `compression` feature)
- Automatic detection in `read_file_data()` — transparent decompression
- Enabled by default (`default = ["compression"]` in Cargo.toml)

### Phase 9 — Hardening ✅

- [x] Volume header validation (block_size power-of-2, total_blocks > 0)
- [x] B-tree header validation (node_size >= 512, root_node sanity)
- [x] Fork reader bounds checking (reject reads past device end)
- [x] Extent record length validation (skip corrupt records)
- [x] GPT CRC validation (header integrity check)
- [x] GPT fallback to MBR on parse failure
- [x] Journal detection and dirty flag reading
- [x] Unicode normalization (NFD→NFC via `unicode-normalization` crate)
- [x] HFSX case-sensitive catalog lookups (via `key_compare_type` from B-tree header)
- [x] Case-insensitive catalog matching for HFS+ volumes
- [ ] Checksum validation (skipped — low impact)

### Phase 10 — Native Windows GUI ✅

- `windows` crate (0.58) for Win32 bindings
- Library crate (`lib.rs`) created so both CLI and GUI share the same HFS+ parser
- Main window with:
  - **Menu bar**: File (Open Image..., Exit) and Help (About)
  - **Toolbar**: Volume combo box, path text field, Up and Extract buttons
  - **List view**: SysListView32 in report mode with Name, Size, Type columns
  - **Status bar**: Volume name, file/folder counts, free space
- Double-click folders to navigate; Up button for parent directory
- Extract button with Save File dialog for exporting files
- Open Image dialog for loading `.img`/`.dmg`/`.raw`/`.dd` files
- File dialogs via raw `comdlg32` FFI
- **Context menu** — right-click on list view shows Extract/Open popup
- **Progress bar** — marquee-style progress bar shown during extraction with status bar feedback
- **Keyboard shortcuts**: Ctrl+O (Open Image), Ctrl+E (Extract), ↑ (Go Up)

### Phase 11 — HFS Original (0x4244) Support ✅

- MDB (Master Directory Block) parser at byte 1024 from volume start
- Signature 0x4244 (`BD`), HFS original boot block + MDB structure
- `HfsMdb` struct with all fields: allocation block size, total/free blocks, catalog/extents file extents, dates
- `HfsExtentDescriptor` (3× `start_block` + `block_count` u16 pairs) for HFS original extent records
- `HfsCatalogFile` extended with `resource_start_block`, `resource_logical_size`, `resource_physical_size`
- Catalog B-tree reader: `HfsCatalogReader` + `HfsCatalogRecord` enum with HFS-original-specific formats
- Extents B-tree: `HfsExtentKey` with 14-byte key (fork_type + file_id + start_block as u16)
- Volume detection: fallback to HFS original when HFS+ header parse fails at offset 1024
- HFS→HFSPlusForkData bridge in `resolve_path_hfs_original()` for API compatibility

### Phase 12 — HFS+ Volume Header Format Fix ✅

- Fixed special-file offsets per Apple spec: `HFSPlusForkData` (80 B) at offsets 112/192/272/352/432 instead of `HfsPlusExtentDescriptor` (8 B) at 112/120/128/136/144
- `VolumeKind::HfsPlus` now uses `Box<VolumeHeader>` to fix clippy `large_enum_variant` warning
- `build_fork_from_fork_data()` uses all 8 inline extents (not just the first)
- Real HFS+ volumes now readable

### Phase 13 — Physical Disk Sector Size Fix ✅

- Volume offset changed from `part.start_lba * device.sector_size()` to `part.start_lba * 512` (partition LBAs are always in 512-byte units per MBR/GPT spec)
- GPT header/entries read at byte offset (`partition_entry_lba * 512`) via new `read_at()` helper instead of device-sector-relative LBA reads
- Fixes physical disk reads on 4Kn (4096-byte sector) drives

### Phase 14 — Apple Partition Map (APM) Support ✅

- `PartitionTable::Apm` variant with `ApmEntry` struct (start_lba, sector_count, name, partition_type, logical_start, logical_count)
- `parse_apm()` reads entries starting at block 1 (byte 512), validates PM signature (0x504D), uses `PMMapBlkCnt` for iteration
- HFS type detection via `is_hfs_apm()` matching `"Apple_HFS"` and `"Apple_HFSX"` type strings
- `find_hfs_partitions()` uses `logical_start`/`logical_count` for APM partition offsets
- Detection falls through to APM when no MBR signature found, or when MBR entries are empty
- 4 unit tests for APM parsing and type detection

## CLI Usage

```
parakses volumes                      List available HFS+ volumes
parakses list <index> /               List root directory
parakses list <index> /path/to/dir    List specific directory
parakses cat <index> /path/to/file    Print file to stdout
parakses extract <index> /src /dst    Extract file to Windows filesystem
```

The `<index>` is the volume number shown by `volumes` (0, 1, 2, ...).

## GUI Usage

```
cargo run --bin parakses_gui
```

(Run as Administrator for physical drive access.)

- Select a volume from the drop-down to browse its root
- Double-click folders to navigate
- Click Extract to save a file to your Windows filesystem
- File → Open Image... to load a raw disk image
- Help → About for version info

## Crate Layout

```
src/
├── lib.rs                # Library crate root; re-exports all public API
├── main.rs               # CLI entry point (all command dispatch)
├── bin/
│   └── parakses_gui.rs   # Native Windows GUI (Win32, windows crate 0.58)
├── cli.rs                # clap argument definitions
├── error.rs              # Custom error types (ParaksesError)
├── volume/
│   ├── mod.rs            # VolumeDiscovery trait
│   ├── partition.rs      # MBR + GPT partition parsing, HFS+ type detection
│   └── windows.rs        # WindowsVolumeEnumerator, HfsPartitionInfo
├── blockio/
│   ├── mod.rs            # BlockDevice trait
│   ├── physical.rs       # PhysicalDrive (Win32 FFI: CreateFile, ReadFile, etc.)
│   ├── filedevice.rs     # FileDevice (raw disk image as block device)
│   └── memfile.rs        # In-memory block device for testing
├── hfs/
│   ├── mod.rs            # HfsVolume struct (open, read, list, resolve path)
│   ├── volume_header.rs  # VolumeHeader, HfsPlusExtentDescriptor, HfsPlusForkData
│   ├── btree/
│   │   ├── mod.rs        # BTreeReader (iterate leaves, search keys)
│   │   ├── node.rs       # NodeDescriptor, HeaderRecord, record offset helpers
│   │   └── key.rs        # Catalog key (raw + typed), Extent key parsing
│   ├── catalog.rs        # CatalogReader (list dirs, find children, parse records)
│   ├── extents.rs        # ExtentsOverflowReader (lookup overflow extents)
│   ├── attribute.rs      # AttributesReader (stub)
│   ├── fork.rs           # ForkReader (allocation block → sector reads)
│   ├── compression.rs    # cmpf detection + zlib decompression
│   └── unicode.rs        # UTF-16BE → String conversion
└── util/
    ├── mod.rs            # Big-endian read helpers (u16, u32, u64)
    └── date.rs           # HFS+ Mac epoch → Unix timestamp conversion
```

## Dependencies

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
anyhow = "1"
thiserror = "2"
log = "0.4"
env_logger = "0.11"
uuid = { version = "1", features = ["v4"] }
flate2 = { version = "1", optional = true }
unicode-normalization = "0.1"
crc32fast = "1"
windows = { version = "0.58", features = ["Win32_Foundation", "Win32_UI_WindowsAndMessaging", "Win32_UI_Controls", "Win32_Graphics_Gdi", "Win32_System_LibraryLoader"] }

[features]
default = ["compression"]
compression = ["flate2"]
```

Win32 API calls in the CLI (`physical.rs`) use direct `unsafe extern "system"` FFI declarations. The GUI (`parakses_gui.rs`) uses the `windows` crate 0.58 for all Win32 bindings plus raw `comdlg32` FFI for file dialogs.

## Requirements

- **Windows 11** (or Windows 10+)
- **Administrator privileges** (required to open `\\.\PhysicalDriveN`)
- **Rust 1.96+** to build

## Key Risks

| Risk | Status |
|------|--------|
| Admin rights on Windows | Documented; tool errors gracefully with `access denied` |
| Non-standard sector size | Handled via `IOCTL_DISK_GET_DRIVE_GEOMETRY` |
| Journaled volume | Detected; dirty flag checked; warning displayed |
| Unicode filename mangling | NFD→NFC normalization applied to displayed names |
| HFS+ compression | Implemented via `cmpf` + zlib decompression |
| HFSX case sensitivity | Handled via B-tree `key_compare_type` flag |
| Corrupt GPT header | Falls back to MBR partition table; CRC validated |

## Bugs Fixed

1. **`gen_test_img.rs`** — B-tree header's `root_node`, `first_leaf_node`, `last_leaf_node` were set to `FIRST_ALLOC+3 = 6` (allocation block number), but node numbers in the B-tree header are indices within the fork (0 = header, 1 = leaf). Changed to `1`. This caused `BTreeReader.read_node_at(6)` to compute offset `6*512=3072` which exceeded the 1024-byte fork, returning empty Vec, and error "Node descriptor too short".

2. **`key.rs`** — `HfsPlusCatalogKeyRaw::parent_id()` and `node_name()` assumed `data` included the keyLength field prefix, but `BTreeReader::read_leaf_node` stores `raw[2..key_len]` (without keyLength). Fixed offsets:
   - `parent_id()`: `self.data[2..]` → `self.data[0..4]`
   - `node_name()`: nameLen at `self.data[6..]` → `self.data[4..6]`, name at `self.data[8..]` → `self.data[6..]`

3. **GUI `LVM_*W` constants** — All list-view message constants used ANSI offsets instead of Wide. `LVM_INSERTCOLUMNW` was `LVM_FIRST + 1` (= `LVM_SETBKCOLOR`), fixed to `+97`. Same for `INSERTITEMW` (`+7`→`+77`), `SETITEMTEXTW` (`+66`→`+116`), `GETITEMTEXTW` (`+75`→`+115`).

4. **`IOCTL_DISK_GET_DRIVE_GEOMETRY` struct** — `DiskGeometry::cylinders` field was `u64` but IOCTL expects a 24-byte `DISK_GEOMETRY` structure. The extra 8 bytes before `media_type` caused the sector size to be read from the wrong offset. Fixed by using the correct #[repr(C)] layout.

5. **HFS+ volume header special file offsets** — `VolumeHeader` parsed `catalog_file`, `extents_file` etc. as `HfsPlusExtentDescriptor` (8 B each) at offsets 120/128/136/144. Apple spec places full `HFSPlusForkData` (80 B each) at offsets 192/272/352/432. The parser was reading garbage from the middle of `allocationFile.logicalSize`. This is why real HFS+ volumes failed to read.

6. **Physical disk sector size mismatch** — Volume offset was `part.start_lba * device.sector_size()`, but partition table LBAs are always in 512-byte units per MBR/GPT spec. On drives with 4096-byte sectors (4Kn), the byte offset was wrong by 8×. Also, GPT header reading used `device.read_sector(offset + 1)` which reads at byte `sector_size` instead of byte 512.

## Current Status

HFS+ and HFS original volumes work end-to-end with both CLI and GUI:

**CLI:**
- `parakses volumes` — lists detected HFS+ partitions on physical drives
- `parakses list 0 / --image image_hfs_1.img` — shows 3 root entries (HFS original)
- `parakses cat 0 /file5.txt --image image_hfs_1.img` — outputs file content
- `parakses extract 0 /file5.txt out.txt --image image_hfs_1.img` — extracts file
- `parakses volumes --image image.img` — detects HFS+ partitions inside a disk image

**GUI:**
- `cargo run --bin parakses_gui` (run as Administrator for physical drive access)
- Physical drives, APM-formatted drives, and images appear in the combo box
- Double-click to browse, right-click context menu for Extract/Open
- Progress bar and status bar feedback during extraction
- Resource fork preservation via Apple Double (`._`) companion files
- Keyboard shortcuts: Ctrl+O (Open Image), Ctrl+E (Extract), ↑ (Go Up)

## Testing Strategy

1. **Unit tests** — 119 unit tests covering catalog parsing, B-tree nodes, fork reads, compression, volume headers, partition tables (MBR/GPT/APM), and integration (HFS+ image)
2. **Integration tests** — `image_hfs_1.img` (bare HFS original) and `image_hfs_plus.img` (HFS+ with MBR) pass end-to-end (list, cat, extract)
3. **Manual acceptance** — Real USB disk on Windows 11

## Next Steps

1. ~~Write unit tests for parsing functions~~ ✅
2. ~~Test the GUI against a real HFS+ USB disk on Windows 11~~ ✅ (volume header + sector size bugs fixed)
3. ~~Consider resource fork extraction~~ ✅ (Apple Double implementation)
4. ~~Add keyboard shortcuts to GUI (Ctrl+O for open image, Ctrl+E for extract)~~ ✅
5. ~~GUI polish: context menu, progress bar, status feedback~~ ✅
6. ~~Apple Partition Map (APM) support~~ ✅
7. **Write support**: see [docs/write-support.md](write-support.md) for detailed analysis. Current recommendation: **not worth pursuing** unless sponsored.

## Out of Scope

- Writing / modifying HFS+ volumes (see [write-support.md](write-support.md) for analysis)
- Journal replay
- Apple File System (APFS)
