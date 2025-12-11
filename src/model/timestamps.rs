// Package model provides timestamp management for entries.

use std::sync::atomic::Ordering;

use crate::time;
use super::Entry;

impl Entry {
    /// Gets the fresh timestamp (when entry was last updated).
    pub fn fresh_at(&self) -> i64 {
        self.updated_at.load(Ordering::Relaxed)
    }

    /// Updates the touched timestamp.
    pub fn touch(&self) {
        self.touched_at.store(time::unix_nano(), Ordering::Relaxed);
    }

    /// Gets the touched timestamp.
    pub fn touched_at(&self) -> i64 {
        self.touched_at.load(Ordering::Relaxed)
    }

    /// Updates the refreshed timestamp.
    pub fn touch_refreshed_at(&self) {
        self.updated_at.store(time::unix_nano(), Ordering::Relaxed);
    }
}

