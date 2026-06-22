# parakses

A native Rust tool to read **HFS+ (Mac OS Extended)** and **HFS (Mac OS Standard)** volumes on Windows 11 — with both a **CLI** and a **GUI**.

Plug in an HFS/HFS+-formatted USB drive or open a raw disk image and browse/extract files — no Mac needed.

## Features

- List available HFS+ and HFS (0x4244) volumes on physical drives or raw disk images
- Browse directory contents with `list` / `ls` (CLI) or point-and-click (GUI)
- Print file contents to stdout with `cat`
- Extract files to the Windows filesystem with `extract` / `cp` / `export`
- Native Windows GUI with list view, combo box, status bar, and menus
- Supports **MBR**, **GPT**, and **Apple Partition Map (APM)** partition tables, plus bare HFS volumes (no partition table)
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

Two binaries are produced:

| Binary | Description |
|--------|-------------|
| `parakses` | Command-line interface |
| `parakses_gui` | Native Windows GUI |

**Requirements:**
- Rust 1.96+
- **Administrator privileges** — required to open `\\.\PhysicalDriveN` for raw disk access (image files do not require admin)
- Windows 10 or 11

## GUI Usage

Launch the GUI (run as Administrator for physical drive access):

```
cargo run --bin parakses_gui
```

The GUI window shows:
- **Volume selector** (top-left) — combo box listing all detected HFS+ volumes on physical drives and any loaded disk images
- **Path bar** (top-center) — shows current directory path on the volume
- **Up / Extract buttons** (top-right)
- **File list** (middle) — detailed list view with Name, Size, Type columns
- **Status bar** (bottom) — displays volume info (name, file/folder count, free space)

### Using the GUI

1. **Select a volume** from the drop-down — the root directory is listed automatically
2. **Double-click a folder** to navigate into it
3. **Right-click** any item for a context menu with Extract / Open
4. **Click Up** to go to the parent directory
5. **Select a file and click Extract** — a progress bar appears, then a Save dialog; pick a destination
6. **File → Open Image...** to load a raw disk image (`.img`, `.dmg`, `.raw`, `.dd`)
7. **Keyboard shortcuts:** `Ctrl+O` (Open Image), `Ctrl+E` (Extract selected), `↑` (Go Up)
8. **Help → About parakses** for version info

## CLI Usage

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
- **HFS original (0x4244)** volumes are supported — bare images without a partition table are detected by the `MDB` signature.
- **Journaled volumes** are detected and their dirty/clean status is shown. A dirty journal means the volume may be in an inconsistent state.
- **HFS+ compressed files** (`.dmg` style zlib compression) are decompressed transparently on extraction.
- **Unicode filenames** are normalized from NFD (HFS+ decomposed form) to NFC for display.

## Architecture

```
┌──────────────────────────────────────────┐
│        parakses (CLI) / parakses_gui     │
│    volumes | list | cat | extract        │
│    or native Win32 GUI window            │
├──────────────────────────────────────────┤
│              Library (lib.rs)            │
│    Shared HFS+ logic used by both CLIs   │
├──────────────────────────────────────────┤
│   HFS+ / HFS Parser (pure Rust)          │
│  Volume Header / MDB | Catalog B-tree    │
│  Extents B-tree | Fork Reader            │
│  Compression (zlib) | Unicode            │
├──────────────────────────────────────────┤
│      Windows Raw Disk Layer              │
│  (Win32 FFI: CreateFile on PhysicalDrive)│
├──────────────────────────────────────────┤
│      Windows Volume Discovery            │
│  (MBR + GPT + APM partition parsing)    │
└──────────────────────────────────────────┘
```

## Project layout

```
src/
├── lib.rs                # Library crate root; re-exports all public API
├── main.rs               # CLI entry point (command dispatch)
├── bin/
│   └── parakses_gui.rs   # Native Windows GUI (Win32, windows crate)
├── cli.rs                # clap argument definitions
├── error.rs              # Custom error types
├── volume/               # Volume discovery (MBR, GPT, Windows enumeration)
├── blockio/              # Block device abstraction (physical drive, file, memory)
├── hfs/                  # HFS+ / HFS filesystem parser
│   ├── btree/            # B-tree engine (generic, used by catalog + extents)
│   ├── catalog.rs        # Directory listing and path resolution (HFS+ & HFS)
│   ├── extents.rs        # Extent overflow lookups
│   ├── fork.rs           # Allocation block → sector reads
│   ├── compression.rs    # HFS+ cmpf decompression
│   ├── unicode.rs        # UTF-16BE decoding, NFD→NFC normalization
│   ├── volume_header.rs  # Volume header / MDB parsing (HFS+ & HFS)
│   └── attribute.rs      # Attributes reader (stub)
└── util/                 # Big-endian helpers, date conversion
```

## License

MIT
