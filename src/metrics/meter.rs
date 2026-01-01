use super::policy::Policy;
use crate::controller::metrics;
use crate::upstream::Policy as UpstreamPolicy;

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
    metrics::inc_cache_hits(value);
}

/// Adds cache misses.
pub fn add_misses(value: u64) {
    metrics::inc_cache_misses(value);
}

/// Sets requests per second.
pub fn set_rps(value: f64) {
    metrics::set_rps(value);
}

/// Sets cache memory usage.
pub fn set_cache_memory(bytes: u64) {
    metrics::set_cache_memory(bytes);
}

/// Adds total requests.
pub fn add_total(value: u64) {
    metrics::inc_total(value);
}

/// Adds errors.
pub fn add_errors(value: u64) {
    metrics::inc_errors(value);
}

/// Adds panics.
pub fn add_panics(value: u64) {
    metrics::inc_panics(value);
}

/// Adds proxied requests.
pub fn add_proxied_num(value: u64) {
    metrics::inc_proxied(value);
}

/// Sets cache length.
pub fn set_cache_length(count: u64) {
    metrics::set_cache_length(count);
}

/// Sets backend policy.
pub fn set_backend_policy(_p: UpstreamPolicy) {
    // Backend policy is not tracked in simple metrics
}

/// Sets lifetime policy.
pub fn set_lifetime_policy(_p: Policy) {
    // Lifetime policy is not tracked in simple metrics
}

/// Sets bypass active status.
pub fn set_is_bypass_active(_is_cache_active: bool) {
    // Bypass status is not tracked in simple metrics
}

/// Sets compression active status.
pub fn set_is_compression_active(_is_active: bool) {
    // Compression status is not tracked in simple metrics
}

/// Sets admission active status.
pub fn set_is_admission_active(_is_active: bool) {
    // Admission status is not tracked in simple metrics
}

/// Sets tracing active status.
pub fn set_is_traces_active(_is_active: bool) {
    // Tracing status is not tracked in simple metrics
}

/// Sets average response times.
pub fn set_avg_response_time(total_dur: f64, cache_dur: f64, proxy_dur: f64, err_dur: f64) {
    metrics::set_avg_response_time(total_dur, cache_dur, proxy_dur, err_dur);
}

/// Adds soft eviction statistics.
pub fn add_soft_eviction_stat_counters(bytes: i64, items: i64, scans: i64) {
    metrics::add_soft_eviction_stats(bytes as u64, items as u64, scans as u64);
}

/// Adds hard eviction statistics.
pub fn add_hard_eviction_stat_counters(
    bytes: i64,
    items: i64,
    adm_allowed: i64,
    adm_not_allowed: i64,
) {
    metrics::add_hard_eviction_stats(
        bytes as u64,
        items as u64,
        adm_allowed as u64,
        adm_not_allowed as u64,
    );
}

/// Adds lifetime statistics.
pub fn add_lifetime_stat_counters(updated: i64, errors: i64, scans: i64, miss: i64, hits: i64) {
    metrics::add_lifetime_stats(
        updated as u64,
        errors as u64,
        scans as u64,
        miss as u64,
        hits as u64,
    );
}
