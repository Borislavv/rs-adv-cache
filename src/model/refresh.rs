use std::sync::atomic::Ordering;

use crate::time;
use crate::config::{Config, ConfigTrait};
use super::Entry;

impl Entry {
    /// Checks that elapsed time is greater than TTL (used in hotpath: GET).
    pub fn is_expired(&self, cfg: &Config) -> bool {
        let ttl = cfg.lifetime()
            .and_then(|l| l.ttl)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        
        // Time since the last successful refresh.
        let updated_at = self.updated_at.load(Ordering::Relaxed);
        let elapsed = time::unix_nano() - updated_at;
        
        elapsed > ttl
    }

    /// Tries to mark the entry as refresh queued.
    pub fn try_mark_refresh_queued(&self) -> bool {
        self.refresh_queued
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    /// Clears the refresh queued flag.
    pub fn clear_refresh_queued(&self) {
        self.refresh_queued.store(false, Ordering::Relaxed);
    }
}

