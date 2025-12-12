// Package evictor provides telemetry for eviction.

use crate::metrics::meter;
use crate::workers::evictor::counters::Counters;
use std::sync::Arc;

/// Logs eviction statistics and updates metrics.
pub fn log_stats(
    name: &str,
    counters: &Arc<Counters>,
) {
    let items = counters.evicted_items.load(std::sync::atomic::Ordering::Relaxed);
    let bytes = counters.evicted_bytes.load(std::sync::atomic::Ordering::Relaxed);
    let scans = counters.scans_total.load(std::sync::atomic::Ordering::Relaxed);
    
    // Update metrics
    meter::add_soft_eviction_stat_counters(bytes, items, scans);
    
    // Log if there's activity
    if items > 0 || bytes > 0 {
        tracing::info!(
            name = %name,
            component = "evictor",
            evicted_items = items,
            evicted_bytes = bytes,
            scans_total = scans,
            "eviction statistics"
        );
    }
}

