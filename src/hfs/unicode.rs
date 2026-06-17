/// Convert UTF-16BE bytes to a Rust String, replacing invalid sequences.
pub fn utf16be_to_string(data: &[u8]) -> String {
    let u16_values: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&u16_values)
}

/// Normalize a filename from HFS+ storage (NFD) to NFC form for Windows.
/// HFS+ stores filenames in UTF-16 using NFD (canonical decomposition).
/// Windows expects NFC (canonical composition) for correct display and path matching.
pub fn normalize_hfs_name(name: &str) -> String {
    normalize_to_nfc(name)
}

/// Convert NFD string to NFC using the unicode-normalization crate.
pub fn normalize_to_nfc(s: &str) -> String {
    unicode_normalization::UnicodeNormalization::nfc(s.chars()).collect()
}

/// Case-insensitive comparison for HFS+ catalog lookups.
/// Uses simple Unicode case folding (to lowercase) for catalog key matching.
pub fn case_fold(s: &str) -> String {
    s.to_lowercase()
}
