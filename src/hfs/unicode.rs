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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf16be_to_string_ascii() {
        let data = b"\x00H\x00e\x00l\x00l\x00o";
        assert_eq!(utf16be_to_string(data), "Hello");
    }

    #[test]
    fn test_utf16be_to_string_empty() {
        assert_eq!(utf16be_to_string(b""), "");
    }

    #[test]
    fn test_utf16be_to_string_unicode() {
        let data = [0x00, 0x63, 0x00, 0x61, 0x00, 0x66, 0x00, 0x65, 0x00, 0xE9];
        assert_eq!(utf16be_to_string(&data), "cafe\u{00E9}");
    }

    #[test]
    fn test_utf16be_to_string_invalid_odd_bytes() {
        let data = b"\x00H\x00e\x00l"; // 3 bytes (odd)
        assert_eq!(utf16be_to_string(data), "Hel");
    }

    #[test]
    fn test_normalize_to_nfc_ascii() {
        assert_eq!(normalize_to_nfc("hello"), "hello");
    }

    #[test]
    fn test_normalize_to_nfc_empty() {
        assert_eq!(normalize_to_nfc(""), "");
    }

    #[test]
    fn test_normalize_to_nfc_nfd_to_nfc() {
        // "é" in NFD is 'e' (U+0065) + combining acute accent (U+0301)
        let nfd = "e\u{0301}";
        let nfc = normalize_to_nfc(nfd);
        assert_eq!(nfc, "\u{00E9}");
    }

    #[test]
    fn test_normalize_to_nfc_already_nfc() {
        assert_eq!(normalize_to_nfc("\u{00E9}"), "\u{00E9}");
    }

    #[test]
    fn test_normalize_hfs_name() {
        let nfd = "cafe\u{0301}";
        let normalized = normalize_hfs_name(nfd);
        assert_eq!(normalized, "caf\u{00E9}");
    }

    #[test]
    fn test_case_fold_ascii() {
        assert_eq!(case_fold("Hello"), "hello");
        assert_eq!(case_fold("HELLO"), "hello");
        assert_eq!(case_fold("hello"), "hello");
    }

    #[test]
    fn test_case_fold_unicode() {
        assert_eq!(case_fold("Caf\u{00E9}"), "caf\u{00E9}");
        assert_eq!(case_fold("CAF\u{00C9}"), "caf\u{00E9}");
    }

    #[test]
    fn test_case_fold_empty() {
        assert_eq!(case_fold(""), "");
    }

    #[test]
    fn test_case_fold_mixed() {
        assert_eq!(case_fold("FiLeNaMe.TxT"), "filename.txt");
    }
}
