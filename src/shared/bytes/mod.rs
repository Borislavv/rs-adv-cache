// Package bytes provides utilities for byte manipulation and formatting.

/// Formats memory size in bytes to a human-readable string.
/// Converts bytes to TB, GB, MB, KB format similar to Go's FmtMem function.
pub fn fmt_mem(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;
    const TB: i64 = GB * 1024;

    match bytes {
        b if b >= TB => {
            let t = b / TB;
            let rem = b % TB;
            format!("{}TB {}GB", t, rem / GB)
        }
        b if b >= GB => {
            let g = b / GB;
            let rem = b % GB;
            format!("{}GB {}MB", g, rem / MB)
        }
        b if b >= MB => {
            let m = b / MB;
            let rem = b % MB;
            format!("{}MB {}KB", m, rem / KB)
        }
        b if b >= KB => {
            let k = b / KB;
            format!("{}KB {}B", k, b % KB)
        }
        b => format!("{}B", b),
    }
}

/// Compares two byte slices for equality.
/// For small slices (< 32 bytes), uses direct comparison.
/// For larger slices, uses a fast hash-based comparison using xxh3.
pub fn is_bytes_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    if a.len() < 32 {
        return a == b;
    }

    use xxhash_rust::xxh3::Xxh3;
    let mut hasher_a = Xxh3::new();
    let mut hasher_b = Xxh3::new();

    // Hash first 8 bytes
    hasher_a.update(&a[..8]);
    hasher_b.update(&b[..8]);

    // Hash middle 8 bytes
    let mid = a.len() / 2;
    hasher_a.update(&a[mid..mid + 8]);
    hasher_b.update(&b[mid..mid + 8]);

    // Hash last 8 bytes
    hasher_a.update(&a[a.len() - 8..]);
    hasher_b.update(&b[b.len() - 8..]);

    hasher_a.digest() == hasher_b.digest()
}

/// Alias for is_bytes_equal to match Go naming convention.
pub fn is_bytes_are_equals(a: &[u8], b: &[u8]) -> bool {
    is_bytes_equal(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt_mem() {
        assert_eq!(fmt_mem(1024), "1KB 0B");
        assert_eq!(fmt_mem(1024 * 1024), "1MB 0KB");
        assert_eq!(fmt_mem(1024 * 1024 * 1024), "1GB 0MB");
    }

    #[test]
    fn test_is_bytes_equal() {
        let a = b"hello world";
        let b = b"hello world";
        let c = b"hello worlX";
        assert!(is_bytes_equal(a, b));
        assert!(!is_bytes_equal(a, c));
    }
}

