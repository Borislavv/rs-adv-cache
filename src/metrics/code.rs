// Status code metrics using atomic counters

use crate::controller::metrics;

/// Increments status code counter.
pub fn inc_status_code(code: u16) {
    metrics::inc_status_code(code);
}

/// Flushes status code counters to metrics.
/// 
/// NOTE: In the simple atomic-based metrics implementation,
/// status codes are already tracked atomically and will be
/// included in the /metrics endpoint automatically.
/// This function is kept for API compatibility but does nothing.
pub fn flush_status_code_counters() {
    // Status codes are already tracked atomically in controller::metrics
    // and will be included in the Prometheus output automatically
}
