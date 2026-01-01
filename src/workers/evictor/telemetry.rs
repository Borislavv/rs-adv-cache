//! Telemetry for eviction.
//
// This file provides metrics/tracing functionality for eviction workers.

use crate::metrics::meter;
use crate::workers::evictor::counters::Counters;
use std::sync::Arc;

/// Logs eviction statistics and updates metrics.
pub fn log_stats(name: &str, counters: &Arc<Counters>) {
    // Swap counters to get values and reset them (prevents double-counting)
    let items = counters
        .evicted_items
        .swap(0, std::sync::atomic::Ordering::Relaxed);
    let bytes = counters
        .evicted_bytes
        .swap(0, std::sync::atomic::Ordering::Relaxed);
    let scans = counters
        .scans_total
        .swap(0, std::sync::atomic::Ordering::Relaxed);

    // Update metrics with accumulated values
    if items > 0 || bytes > 0 || scans > 0 {
        meter::add_soft_eviction_stat_counters(bytes, items, scans);
    }

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
