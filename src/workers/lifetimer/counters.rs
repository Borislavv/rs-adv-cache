// Package lifetimer provides counters for lifetime management.

use std::sync::atomic::{AtomicI64, Ordering};

/// Counters for lifetime management operations.
pub struct Counters {
    /// Successful refresh operations.
    pub success_updates: AtomicI64,
    /// Failed refresh operations.
    pub error_updates: AtomicI64,
    /// Total scan operations.
    pub scans_total: AtomicI64,
    /// Scans that found expired entries.
    pub scans_hit: AtomicI64,
    /// Scans that found no expired entries.
    pub scans_miss: AtomicI64,
}

impl Counters {
    /// Creates new counters.
    pub fn new() -> Self {
        Self {
            success_updates: AtomicI64::new(0),
            error_updates: AtomicI64::new(0),
            scans_total: AtomicI64::new(0),
            scans_hit: AtomicI64::new(0),
            scans_miss: AtomicI64::new(0),
        }
    }

    /// Resets all counters and returns their previous values.
    pub fn reset(&self) -> (i64, i64, i64, i64, i64) {
        let updated = self.success_updates.swap(0, Ordering::Relaxed);
        let errors = self.error_updates.swap(0, Ordering::Relaxed);
        let scans = self.scans_total.swap(0, Ordering::Relaxed);
        let hits = self.scans_hit.swap(0, Ordering::Relaxed);
        let miss = self.scans_miss.swap(0, Ordering::Relaxed);
        (updated, errors, scans, miss, hits)
    }
}

impl Default for Counters {
    fn default() -> Self {
        Self::new()
    }
}

