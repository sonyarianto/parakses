# parakses

A native Rust CLI tool to read **HFS+ (Mac OS Extended)** volumes on Windows 11.

Plug in an HFS+-formatted USB drive or open a raw disk image and browse/extract files — no Mac needed.

## Features

- List available HFS+ volumes on physical drives or raw disk images
- Browse directory contents with `list` / `ls`
- Print file contents to stdout with `cat`
- Extract files to the Windows filesystem with `extract` / `cp` / `export`
- Supports MBR and GPT partition tables
- HFS+ compression (zlib) decompression
- Unicode normalization (NFD → NFC)
- HFSX case-sensitive volumes
- Journal detection and dirty flag checking

## Installation

```bash
git clone <repo>
cd parakses
cargo build --release
```

**Requirements:**
- Rust 1.96+
- **Administrator privileges** — required to open `\\.\PhysicalDriveN` for raw disk access
- Windows 10 or 11

## Usage

### 1. List available volumes

Scan all physical drives for HFS+ partitions:

```
parakses volumes
```

Example output:
```
[0] PhysicalDrive2
  Partition 0: start LBA 4032, 31256784 sectors (~15262 MB) — My Backup
```

The number in brackets `[0]` is the **volume index** — you'll use it in subsequent commands.

### 2. List directory contents

List the root directory of volume 0:

```
parakses list 0 /
```

List a subdirectory:

```
parakses list 0 /Documents
```

Alias: `parakses ls 0 /`

Sample output:
```
Volume: My Backup
  Signature:  0x482b
  Version:    4
  Type:       HFS+
  Block size: 4096 bytes
  Capacity:   3907098 blocks (15262 MB)
  Free:       1284033 blocks (5015 MB)
  Files:      842
  Folders:    68
  Write count: 39418
  Journal:    present (clean)

Directory listing:
  [DIR]          0  Documents
  [DIR]          0  Photos
  [   ]   52428800  movie.mov
```

### 3. Print a file to terminal

Useful for text files (output is binary-safe):

```
parakses cat 0 /Documents/notes.txt
```

### 4. Extract a file

Copy a file from the HFS+ volume to your Windows filesystem:

```
parakses extract 0 /Photos/vacation.jpg C:\Users\You\Pictures\vacation.jpg
```

Aliases: `parakses cp 0 /file.txt out.txt` or `parakses export 0 /file.txt out.txt`

### Working with disk images

If you have a raw disk image (`.img`, `.dmg` converted to raw), use `--image` / `-f`:

```
parakses --image backup.img volumes
parakses --image backup.img list 0 /
parakses --image backup.img cat 0 /hello.txt
parakses --image backup.img extract 0 /file.txt out.txt
```

For multi-partition images, select a partition with `--partition` / `-p`:

```
parakses --image multi.img --partition 1 list 0 /
```

### Important notes

- **Run as Administrator** — physical drive access requires elevation. Without it, you'll get "access denied" errors. If you're using `--image`, admin is not required.
- **Volume index** — the number shown by `volumes` (e.g. `[0]`) is used as the first argument to `list`, `cat`, `extract`.
- **Paths** use forward slashes (`/`) and are absolute within the HFS+ volume.
- **Journaled volumes** are detected and their dirty/clean status is shown. A dirty journal means the volume may be in an inconsistent state.
- **HFS+ compressed files** (`.dmg` style zlib compression) are decompressed transparently on extraction.
- **Unicode filenames** are normalized from NFD (HFS+ decomposed form) to NFC for display.

## Architecture

```
┌─────────────────────────────────────────┐
│          parakses.exe <command>         │
│    volumes | list | cat | extract       │
├─────────────────────────────────────────┤
│          HFS+ Parser (pure Rust)        │
│  Volume Header | Catalog B-tree         │
│  Extents B-tree | Fork Reader           │
│  Compression (zlib) | Unicode           │
├─────────────────────────────────────────┤
│      Windows Raw Disk Layer             │
│  (Win32 FFI: CreateFile on PhysicalDrive)│
├─────────────────────────────────────────┤
│      Windows Volume Discovery           │
│  (MBR + GPT partition table parsing)    │
└─────────────────────────────────────────┘
```

## Project layout

```
src/
├── main.rs              # CLI entry point (command dispatch)
├── cli.rs               # clap argument definitions
├── error.rs             # Custom error types
├── volume/              # Volume discovery (MBR, GPT, Windows enumeration)
├── blockio/             # Block device abstraction (physical drive, file, memory)
├── hfs/                 # HFS+ filesystem parser
│   ├── btree/           # B-tree engine (generic, used by catalog + extents)
│   ├── catalog.rs       # Directory listing and path resolution
│   ├── extents.rs       # Extent overflow lookups
│   ├── fork.rs          # Allocation block → sector reads
│   ├── compression.rs   # HFS+ cmpf decompression
│   ├── unicode.rs       # UTF-16BE decoding, NFD→NFC normalization
│   ├── volume_header.rs # Volume header parsing
│   └── attribute.rs     # Attributes reader (stub)
└── util/                # Big-endian helpers, date conversion
```

## License

MIT
