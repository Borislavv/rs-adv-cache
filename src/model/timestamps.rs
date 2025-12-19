//! Timestamp management for entries.
//

use std::sync::atomic::Ordering;

use super::Entry;
use crate::time;

impl Entry {
    /// Gets the fresh timestamp (when entry was last updated).
    pub fn fresh_at(&self) -> i64 {
        self.0.updated_at.load(Ordering::Relaxed)
    }

    /// Updates the touched timestamp.
    pub fn touch(&self) {
        self.0.touched_at.store(time::unix_nano(), Ordering::Relaxed);
    }

    /// Gets the touched timestamp.
    pub fn touched_at(&self) -> i64 {
        self.0.touched_at.load(Ordering::Relaxed)
    }

    /// Updates the refreshed timestamp.
    pub fn touch_refreshed_at(&self) {
        self.0.updated_at.store(time::unix_nano(), Ordering::Relaxed);
    }

    /// Untouches the refreshed timestamp (sets it to past).
    pub fn untouch_refreshed_at(&self) {
        // Calculate TTL in nanoseconds
        // Use refresh lifetime rule for TTL
        let ttl_nanos = self.0.rule
            .refresh
            .as_ref()
            .and_then(|r| r.ttl)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        self.0.updated_at
            .store(time::unix_nano() - ttl_nanos, Ordering::Relaxed);
    }

    /// Helper to force a specific refreshed timestamp (used in tests).
    #[allow(dead_code)]
    pub fn set_refreshed_at_for_tests(&self, ts: i64) {
        self.0.updated_at.store(ts, Ordering::Relaxed);
    }
}
