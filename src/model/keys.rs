// Package model provides key building and fingerprint comparison.

use xxhash_rust::xxh3::Xxh3;

use super::Entry;

impl Entry {
    /// Gets the cache key.
    pub fn key(&self) -> u64 {
        self.key
    }

    /// Builds the cache key from filtered queries and headers.
    pub(crate) fn build_key(&mut self, filtered_queries: &[(Vec<u8>, Vec<u8>)], filtered_headers: &[(Vec<u8>, Vec<u8>)]) {
        // Calculate buffer size
        let mut buf_len = self.rule.path_bytes.as_ref().map(|p| p.len()).unwrap_or(0);
        for (k, v) in filtered_queries {
            buf_len += k.len() + v.len();
        }
        for (k, v) in filtered_headers {
            buf_len += k.len() + v.len();
        }

        // Build buffer
        let mut buf = Vec::with_capacity(buf_len);
        if let Some(ref path_bytes) = self.rule.path_bytes {
            buf.extend_from_slice(path_bytes);
        }
        for (k, v) in filtered_queries {
            buf.extend_from_slice(k);
            buf.extend_from_slice(v);
        }
        for (k, v) in filtered_headers {
            buf.extend_from_slice(k);
            buf.extend_from_slice(v);
        }

        // Calculate hash using xxh3
        let mut hasher = Xxh3::new();
        hasher.update(&buf);
        
        // Calculate 64-bit key
        self.key = hasher.digest();
        
        // Calculate 128-bit fingerprint
        let fingerprint = hasher.digest128();
        self.fingerprint_hi = (fingerprint >> 64) as u64;
        self.fingerprint_lo = fingerprint as u64;
    }

    /// Checks if two entries have the same fingerprint.
    pub fn is_the_same_fingerprint(&self, other: &Entry) -> bool {
        self.fingerprint_hi == other.fingerprint_hi && self.fingerprint_lo == other.fingerprint_lo
    }
}

