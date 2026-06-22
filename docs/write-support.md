# HFS+ / HFS Write Support — Design & Evaluation

## Goal

Enable **writing files to HFS+ volumes** from Windows 11 — create files, write data forks, create directories, delete files. Currently parakses is read-only.

## Is It Worth It?

### Arguments for

- **Completes the use case**: read *and* write Mac disks from Windows without third-party tools
- **Unique value**: no free Windows tool can write HFS+ reliably (MacDrive is paid, `hfsplus-tools` on Linux only)
- **Recovery scenario**: copy files back to a fixed volume after repair

### Arguments against

- **6–8 weeks of work** for a feature most users won't need (reading/extracting is the primary use case)
- **High risk of data loss**: B-tree mutation bugs can silently corrupt a volume
- **Journal handling**: most macOS volumes are journaled; writes without journal replay require disabling journaling, which macOS may re-enable
- **Maintenance burden**: write path adds significant complexity to the codebase

### Verdict

**Not worth it for the current purpose.** The read-only tool solves the core problem ("I have a Mac disk and need files off it on Windows"). Write support should only be pursued if:

1. A specific user/company needs it and is willing to sponsor development
2. The tool gains significant Windows user adoption and write is the most-requested feature
3. Someone contributes it as a separate open-source project that this library can integrate

---

## If Pursued: Architecture & Plan

### BlockDevice write API

Add to `src/blockio/mod.rs`:

```rust
pub trait BlockDevice {
    // existing
    fn sector_size(&self) -> u32;
    fn total_sectors(&self) -> u64;
    fn read_sector(&self, lba: u64) -> io::Result<Vec<u8>>;

    // new
    fn write_sector(&self, lba: u64, data: &[u8]) -> io::Result<()>;
    fn write_sectors(&self, start_lba: u64, data: &[u8]) -> io::Result<()>;
}
```

Implementations:
- **FileDevice** — `seek_write()` via `std::os::windows::fs::FileExt`
- **PhysicalDrive** — `SetFilePointerEx` + `WriteFile` (add `GENERIC_WRITE` to `CreateFileW`)
- **MemFile** — in-memory mutation (for tests)

### Phase 1 — Non-Journaled HFS+ Write (minimum viable)

#### 1. Allocation bitmap mutation

- Read allocation file pages, locate free blocks via bitmap scan
- `bitmap_alloc(count)` — find `count` consecutive free blocks, mark allocated, return `(start_block, block_count)`
- `bitmap_free(start_block, block_count)` — clear bits
- Update volume header `free_blocks` and `next_allocation` hints
- **Risk**: no rollback if write fails partway

#### 2. Extent allocation

- For a write of `N` bytes, compute required allocation blocks: `blocks = ceil(N / block_size)`
- Call `bitmap_alloc()` to get a contiguous run
- If the file already has extent records:
  - If the last extent is physically adjacent, extend it (just update block_count)
  - Otherwise, append a new extent to the fork data extents
  - If > 8 extents, write an overflow extent record to the ExtentsOverflow B-tree
- Update `HfsPlusForkData.logical_size` and `total_blocks`

#### 3. Fork data writing

- Write raw bytes to the allocated blocks using `write_sector()`
- After all blocks written, flush and verify

#### 4. Catalog B-tree mutation (CREATE)

Insert a new catalog leaf record:

- Traverse the B-tree using existing search logic to find the insertion point (leaf node + record index)
- Insert the new record entry in the target leaf node:
  - Shift existing records right
  - Write the new catalog key + record data
  - Update the record offset table
- Write the modified leaf node back via `ForkReader` extended with write support

**No node splitting** (Phase 1 constraint):
- Before insert, check if the leaf node has enough free space
- If not, fail with "B-tree node full" error
- This works for most small/medium volumes (< 10,000 files) where B-tree nodes are typically less than 50% full

#### 5. ExtentsOverflow B-tree mutation

If > 8 extents are needed, insert an overflow extent record into the ExtentsOverflow B-tree. Same approach as catalog B-tree insert.

#### 6. Volume header update

After all mutations succeed:
- Update `free_blocks`
- Update `modify_date`
- Update `write_count`

### Phase 2 — Journal Support (if needed later)

- Read journal from `journal_info_block` offset in volume header
- Parse journal structure (journal header, transaction start/end/commit blocks)
- Replay pending transactions before write
- Wrap writes in a journal transaction
- **Complexity**: very high. The HFS+ journal is circular buffer with its own state machine

### Phase 3 — B-tree Node Splitting

- Remove the "no split" constraint
- Implement leaf node split: allocate new leaf, redistribute records, update forward/backward links in the node descriptor, insert index record in parent
- Recursively split index nodes up to the root
- Tree growth: write a new root node when current root splits

## What won't be implemented

| Feature | Reason |
|---------|--------|
| Resource fork writes | Apple Double sidecar approach is fragile for write-back |
| Extended attributes / ACLs | Extremely rare on HFS+ volumes in practice |
| Defragmentation | Out of scope |
| Journaled volume writes (Phase 1) | Requires full journal implementation |
| HFS original write | Very old format, almost no real-world use for write |
| File truncation / resize | Just use the delete+recreate pattern |
| Rename | Requires updating catalog key which means delete+reinsert |

## Testing strategy

1. **Unit tests**: bitmap operations, extent allocation, B-tree node mutation in isolation
2. **Fake device tests**: use `MemFile` with a known-good HFS+ image; write a file, verify via read-only parser
3. **Read-after-write verification**: write a file, then extract it and compare checksums
4. **Fault injection**: test partial-write recovery (power-loss simulation)
5. **Manual**: `hfsplus` Linux tool can `fsck` the image after writes to verify integrity

## Effort estimate (if pursued)

| Task | Time |
|------|------|
| BlockDevice write API + all implementations | 2 days |
| Allocation bitmap read/write | 3 days |
| Fork extent write (inline + overflow) | 5 days |
| Catalog B-tree insert (no split) | 5 days |
| ExtentsOverflow B-tree insert | 2 days |
| Volume header update | 1 day |
| File/directory create/delete/rename (no split) | 3 days |
| **Phase 1 total** | **~3 weeks** |
| Journal support (Phase 2) | 1–2 weeks |
| B-tree node splitting (Phase 3) | 1–2 weeks |
| Testing & hardening | 2+ weeks |
| **Full total** | **~8 weeks** |
