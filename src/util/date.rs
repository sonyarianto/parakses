/// HFS+ stores dates as unsigned 32-bit seconds since midnight January 1, 1904 (local time).
/// This is the classic Mac epoch.
const MAC_EPOCH_OFFSET: u64 = 2_082_844_800; // seconds between 1904-01-01 and 1970-01-01

/// Convert an HFS+ date to a Unix timestamp (seconds since 1970-01-01).
pub fn hfs_date_to_unix(hfs_date: u32) -> u64 {
    u64::from(hfs_date).wrapping_sub(MAC_EPOCH_OFFSET)
}
