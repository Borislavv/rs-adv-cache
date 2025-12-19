//! Counters for eviction statistics.
//

use std::sync::atomic::AtomicI64;
use std::sync::Arc;

/// Counters for eviction statistics.
pub struct Counters {
    pub evicted_items: Arc<AtomicI64>,
    pub evicted_bytes: Arc<AtomicI64>,
    pub scans_total: Arc<AtomicI64>,
}

impl Counters {
    /// Creates a new counters instance.
    pub fn new() -> Self {
        Self {
            evicted_items: Arc::new(AtomicI64::new(0)),
            evicted_bytes: Arc::new(AtomicI64::new(0)),
            scans_total: Arc::new(AtomicI64::new(0)),
        }
    }
}

impl Default for Counters {
    fn default() -> Self {
        Self::new()
    }
}
