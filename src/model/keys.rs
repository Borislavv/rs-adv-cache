//! Key building and fingerprint comparison.
//

use super::Entry;

impl Entry {
    /// Gets the cache key.
    pub fn key(&self) -> u64 {
        self.0.key
    }

    /// Checks if two entries have the same fingerprint.
    pub fn is_the_same_fingerprint(&self, other: &Entry) -> bool {
        self.0.fingerprint_hi == other.0.fingerprint_hi 
            && self.0.fingerprint_lo == other.0.fingerprint_lo
    }
}
