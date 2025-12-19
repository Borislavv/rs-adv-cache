use super::policy::Policy;
use crate::upstream::Policy as UpstreamPolicy;

// Metric name constants
pub const AVG_TOTAL_DURATION: &str = "avg_duration_ns";
pub const AVG_CACHE_DURATION: &str = "avg_cache_duration_ns";
pub const AVG_PROXY_DURATION: &str = "avg_proxy_duration_ns";
pub const AVG_ERROR_DURATION: &str = "avg_error_duration_ns";

pub const RPS: &str = "rps";
pub const TOTAL: &str = "total";
pub const ERRORED: &str = "errors";
pub const PANICKED: &str = "panics";
pub const PROXIED: &str = "proxies";
pub const HITS: &str = "cache_hits";
pub const MISSES: &str = "cache_misses";
pub const MAP_MEMORY_USAGE_METRIC_NAME: &str = "cache_memory_usage";
pub const MAP_LENGTH: &str = "cache_length";

pub const TOTAL_SOFT_EVICTIONS: &str = "soft_evicted_total_items";
pub const TOTAL_SOFT_BYTES_EVICTED: &str = "soft_evicted_total_bytes";
pub const TOTAL_SOFT_SCANS: &str = "soft_evicted_total_scans";

pub const TOTAL_HARD_EVICTIONS: &str = "hard_evicted_total_items";
pub const TOTAL_HARD_BYTES_EVICTED: &str = "hard_evicted_total_bytes";

pub const TOTAL_ADM_ALLOWED: &str = "admission_allowed";
pub const TOTAL_ADM_NOT_ALLOWED: &str = "admission_not_allowed";

pub const REFRESHER_UPDATED: &str = "refresh_updated";
pub const REFRESHER_ERRORS: &str = "refresh_errors";
pub const REFRESHER_SCANS: &str = "refresh_scans";
pub const REFRESHER_HITS: &str = "refresh_hits";
pub const REFRESHER_MISS: &str = "refresh_miss";

pub const BACKEND_POLICY: &str = "backend_policy";
pub const LIFETIME_POLICY: &str = "lifetime_policy";

pub const IS_BYPASS_ACTIVE: &str = "is_bypass_active";
pub const IS_COMPRESSION_ACTIVE: &str = "is_compression_active";
pub const IS_TRACES_ACTIVE: &str = "is_traces_active";
pub const IS_ADMISSION_ACTIVE: &str = "is_admission_active";


/// Adds cache hits.
pub fn add_hits(value: u64) {
    metrics::counter!(HITS, value);
}

/// Adds cache misses.
pub fn add_misses(value: u64) {
    metrics::counter!(MISSES, value);
}

/// Sets requests per second.
pub fn set_rps(value: f64) {
    metrics::gauge!(RPS, value);
}

/// Sets cache memory usage.
pub fn set_cache_memory(bytes: u64) {
    // Use gauge for absolute values (memory usage)
    metrics::gauge!(MAP_MEMORY_USAGE_METRIC_NAME, bytes as f64);
}

/// Adds total requests.
pub fn add_total(value: u64) {
    metrics::counter!(TOTAL, value);
}

/// Adds errors.
pub fn add_errors(value: u64) {
    metrics::counter!(ERRORED, value);
}

/// Adds panics.
pub fn add_panics(value: u64) {
    metrics::counter!(PANICKED, value);
}

/// Adds proxied requests.
pub fn add_proxied_num(value: u64) {
    metrics::counter!(PROXIED, value);
}

/// Sets cache length.
pub fn set_cache_length(count: u64) {
    // Use gauge for set operations (absolute values)
    metrics::gauge!(MAP_LENGTH, count as f64);
}

/// Sets backend policy.
pub fn set_backend_policy(p: UpstreamPolicy) {
    metrics::gauge!(BACKEND_POLICY, p.to_u64() as f64);
}

/// Sets lifetime policy.
pub fn set_lifetime_policy(p: Policy) {
    metrics::gauge!(LIFETIME_POLICY, p.to_u64() as f64);
}

/// Sets bypass active status.
pub fn set_is_bypass_active(is_cache_active: bool) {
    let is_bypass = if !is_cache_active { 1.0 } else { 0.0 };
    metrics::gauge!(IS_BYPASS_ACTIVE, is_bypass);
}

/// Sets compression active status.
pub fn set_is_compression_active(is_active: bool) {
    let is_compression = if is_active { 1.0 } else { 0.0 };
    metrics::gauge!(IS_COMPRESSION_ACTIVE, is_compression);
}

/// Sets admission active status.
pub fn set_is_admission_active(is_active: bool) {
    let is_admission_active = if is_active { 1.0 } else { 0.0 };
    metrics::gauge!(IS_ADMISSION_ACTIVE, is_admission_active);
}

/// Sets tracing active status.
pub fn set_is_traces_active(is_active: bool) {
    let is_traces_active = if is_active { 1.0 } else { 0.0 };
    metrics::gauge!(IS_TRACES_ACTIVE, is_traces_active);
}

/// Sets average response times.
pub fn set_avg_response_time(total_dur: f64, cache_dur: f64, proxy_dur: f64, err_dur: f64) {
    metrics::gauge!(AVG_TOTAL_DURATION, total_dur);
    metrics::gauge!(AVG_CACHE_DURATION, cache_dur);
    metrics::gauge!(AVG_PROXY_DURATION, proxy_dur);
    metrics::gauge!(AVG_ERROR_DURATION, err_dur);
}


/// Adds soft eviction statistics.
pub fn add_soft_eviction_stat_counters(bytes: i64, items: i64, scans: i64) {
    metrics::counter!(TOTAL_SOFT_EVICTIONS, items as u64);
    metrics::counter!(TOTAL_SOFT_BYTES_EVICTED, bytes as u64);
    metrics::counter!(TOTAL_SOFT_SCANS, scans as u64);
}

/// Adds hard eviction statistics.
pub fn add_hard_eviction_stat_counters(
    bytes: i64,
    items: i64,
    adm_allowed: i64,
    adm_not_allowed: i64,
) {
    metrics::counter!(TOTAL_HARD_EVICTIONS, items as u64);
    metrics::counter!(TOTAL_HARD_BYTES_EVICTED, bytes as u64);
    metrics::counter!(TOTAL_ADM_ALLOWED, adm_allowed as u64);
    metrics::counter!(TOTAL_ADM_NOT_ALLOWED, adm_not_allowed as u64);
}

/// Adds lifetime statistics.
pub fn add_lifetime_stat_counters(updated: i64, errors: i64, scans: i64, miss: i64, hits: i64) {
    metrics::counter!(REFRESHER_UPDATED, updated as u64);
    metrics::counter!(REFRESHER_ERRORS, errors as u64);
    metrics::counter!(REFRESHER_SCANS, scans as u64);
    metrics::counter!(REFRESHER_HITS, hits as u64);
    metrics::counter!(REFRESHER_MISS, miss as u64);
}
