//! Metrics controller with simple atomic counters and Prometheus formatting.

use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use crate::http::Controller;

pub const PROMETHEUS_METRICS_PATH: &str = "/metrics";

// Atomic counters for metrics
static CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static TOTAL_REQUESTS: AtomicU64 = AtomicU64::new(0);
static ERRORED_REQUESTS: AtomicU64 = AtomicU64::new(0);
static PROXIED_REQUESTS: AtomicU64 = AtomicU64::new(0);
static PANICKED_REQUESTS: AtomicU64 = AtomicU64::new(0);

// Gauges (f64 stored as u64 bits for atomic operations)
static RPS: AtomicU64 = AtomicU64::new(0);
static CACHE_MEMORY_USAGE: AtomicU64 = AtomicU64::new(0);
static PROCESS_PHYSICAL_MEMORY_USAGE: AtomicU64 = AtomicU64::new(0);
static CACHE_LENGTH: AtomicU64 = AtomicU64::new(0);
static AVG_TOTAL_DURATION: AtomicU64 = AtomicU64::new(0);
static AVG_CACHE_DURATION: AtomicU64 = AtomicU64::new(0);
static AVG_PROXY_DURATION: AtomicU64 = AtomicU64::new(0);
static AVG_ERROR_DURATION: AtomicU64 = AtomicU64::new(0);
static CPU_USAGE_CORES: AtomicU64 = AtomicU64::new(0);

// Eviction metrics
static SOFT_EVICTIONS: AtomicU64 = AtomicU64::new(0);
static SOFT_BYTES_EVICTED: AtomicU64 = AtomicU64::new(0);
static SOFT_SCANS: AtomicU64 = AtomicU64::new(0);
static HARD_EVICTIONS: AtomicU64 = AtomicU64::new(0);
static HARD_BYTES_EVICTED: AtomicU64 = AtomicU64::new(0);
static ADMISSION_ALLOWED: AtomicU64 = AtomicU64::new(0);
static ADMISSION_NOT_ALLOWED: AtomicU64 = AtomicU64::new(0);

// Refresh metrics
static REFRESH_UPDATED: AtomicU64 = AtomicU64::new(0);
static REFRESH_ERRORS: AtomicU64 = AtomicU64::new(0);
static REFRESH_SCANS: AtomicU64 = AtomicU64::new(0);
static REFRESH_HITS: AtomicU64 = AtomicU64::new(0);
static REFRESH_MISS: AtomicU64 = AtomicU64::new(0);

// Status code counters (0-599)
static STATUS_CODE_COUNTERS: OnceLock<Vec<AtomicU64>> = OnceLock::new();

fn get_status_code_counters() -> &'static Vec<AtomicU64> {
    STATUS_CODE_COUNTERS.get_or_init(|| {
        (0..600).map(|_| AtomicU64::new(0)).collect()
    })
}

/// Increments cache hits counter.
pub fn inc_cache_hits(value: u64) {
    CACHE_HITS.fetch_add(value, Ordering::Relaxed);
}

/// Increments cache misses counter.
pub fn inc_cache_misses(value: u64) {
    CACHE_MISSES.fetch_add(value, Ordering::Relaxed);
}

/// Sets requests per second.
pub fn set_rps(value: f64) {
    RPS.store(value.to_bits(), Ordering::Relaxed);
}

/// Sets cache memory usage (logical memory used by cache data structures).
pub fn set_cache_memory(bytes: u64) {
    CACHE_MEMORY_USAGE.store(bytes, Ordering::Relaxed);
}

/// Sets process physical memory usage (RSS - Resident Set Size from system).
pub fn set_process_physical_memory(bytes: u64) {
    PROCESS_PHYSICAL_MEMORY_USAGE.store(bytes, Ordering::Relaxed);
}

/// Increments total requests counter.
pub fn inc_total(value: u64) {
    TOTAL_REQUESTS.fetch_add(value, Ordering::Relaxed);
}

/// Increments errored requests counter.
pub fn inc_errors(value: u64) {
    ERRORED_REQUESTS.fetch_add(value, Ordering::Relaxed);
}

/// Increments panicked requests counter.
pub fn inc_panics(value: u64) {
    PANICKED_REQUESTS.fetch_add(value, Ordering::Relaxed);
}

/// Increments proxied requests counter.
pub fn inc_proxied(value: u64) {
    PROXIED_REQUESTS.fetch_add(value, Ordering::Relaxed);
}

/// Sets cache length.
pub fn set_cache_length(count: u64) {
    CACHE_LENGTH.store(count, Ordering::Relaxed);
}

/// Sets average response times.
pub fn set_avg_response_time(total_dur: f64, cache_dur: f64, proxy_dur: f64, err_dur: f64) {
    AVG_TOTAL_DURATION.store(total_dur.to_bits(), Ordering::Relaxed);
    AVG_CACHE_DURATION.store(cache_dur.to_bits(), Ordering::Relaxed);
    AVG_PROXY_DURATION.store(proxy_dur.to_bits(), Ordering::Relaxed);
    AVG_ERROR_DURATION.store(err_dur.to_bits(), Ordering::Relaxed);
}

/// Sets CPU usage in cores (number of CPU cores utilized).
pub fn set_cpu_usage_cores(cores: f64) {
    CPU_USAGE_CORES.store(cores.to_bits(), Ordering::Relaxed);
}

/// Adds soft eviction statistics.
pub fn add_soft_eviction_stats(bytes: u64, items: u64, scans: u64) {
    SOFT_EVICTIONS.fetch_add(items, Ordering::Relaxed);
    SOFT_BYTES_EVICTED.fetch_add(bytes, Ordering::Relaxed);
    SOFT_SCANS.fetch_add(scans, Ordering::Relaxed);
}

/// Adds hard eviction statistics.
pub fn add_hard_eviction_stats(bytes: u64, items: u64, adm_allowed: u64, adm_not_allowed: u64) {
    HARD_EVICTIONS.fetch_add(items, Ordering::Relaxed);
    HARD_BYTES_EVICTED.fetch_add(bytes, Ordering::Relaxed);
    ADMISSION_ALLOWED.fetch_add(adm_allowed, Ordering::Relaxed);
    ADMISSION_NOT_ALLOWED.fetch_add(adm_not_allowed, Ordering::Relaxed);
}

/// Adds lifetime statistics.
pub fn add_lifetime_stats(updated: u64, errors: u64, scans: u64, miss: u64, hits: u64) {
    REFRESH_UPDATED.fetch_add(updated, Ordering::Relaxed);
    REFRESH_ERRORS.fetch_add(errors, Ordering::Relaxed);
    REFRESH_SCANS.fetch_add(scans, Ordering::Relaxed);
    REFRESH_HITS.fetch_add(hits, Ordering::Relaxed);
    REFRESH_MISS.fetch_add(miss, Ordering::Relaxed);
}

/// Increments status code counter.
pub fn inc_status_code(code: u16) {
    if code < 600 {
        let counters = get_status_code_counters();
        if let Some(counter) = counters.get(code as usize) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Formats metrics in Prometheus format.
/// Optimized to reduce String allocations by using push_str with to_string() for numbers.
fn format_prometheus_metrics() -> String {
    // Pre-allocate String with estimated capacity (typical metrics output is ~2-4KB)
    let mut output = String::with_capacity(4096);
    
    // Counters
    output.push_str("# HELP cache_hits Total number of cache hits\n");
    output.push_str("# TYPE cache_hits counter\n");
    output.push_str("cache_hits ");
    output.push_str(&CACHE_HITS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP cache_misses Total number of cache misses\n");
    output.push_str("# TYPE cache_misses counter\n");
    output.push_str("cache_misses ");
    output.push_str(&CACHE_MISSES.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP total Total number of requests\n");
    output.push_str("# TYPE total counter\n");
    output.push_str("total ");
    output.push_str(&TOTAL_REQUESTS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP errors Total number of errors\n");
    output.push_str("# TYPE errors counter\n");
    output.push_str("errors ");
    output.push_str(&ERRORED_REQUESTS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP proxies Total number of proxied requests\n");
    output.push_str("# TYPE proxies counter\n");
    output.push_str("proxies ");
    output.push_str(&PROXIED_REQUESTS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP panics Total number of panics\n");
    output.push_str("# TYPE panics counter\n");
    output.push_str("panics ");
    output.push_str(&PANICKED_REQUESTS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    // Gauges
    output.push_str("# HELP rps Requests per second\n");
    output.push_str("# TYPE rps gauge\n");
    output.push_str("rps ");
    output.push_str(&f64::from_bits(RPS.load(Ordering::Relaxed)).to_string());
    output.push('\n');
    
    output.push_str("# HELP cache_memory_usage Cache logical memory usage in bytes (memory used by cache data structures)\n");
    output.push_str("# TYPE cache_memory_usage gauge\n");
    output.push_str("cache_memory_usage ");
    output.push_str(&CACHE_MEMORY_USAGE.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP process_physical_memory_usage Process physical memory usage in bytes (RSS - Resident Set Size from system)\n");
    output.push_str("# TYPE process_physical_memory_usage gauge\n");
    output.push_str("process_physical_memory_usage ");
    output.push_str(&PROCESS_PHYSICAL_MEMORY_USAGE.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP cache_length Number of items in cache\n");
    output.push_str("# TYPE cache_length gauge\n");
    output.push_str("cache_length ");
    output.push_str(&CACHE_LENGTH.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP avg_duration_ns Average total duration in nanoseconds\n");
    output.push_str("# TYPE avg_duration_ns gauge\n");
    output.push_str("avg_duration_ns ");
    output.push_str(&f64::from_bits(AVG_TOTAL_DURATION.load(Ordering::Relaxed)).to_string());
    output.push('\n');
    
    output.push_str("# HELP avg_cache_duration_ns Average cache duration in nanoseconds\n");
    output.push_str("# TYPE avg_cache_duration_ns gauge\n");
    output.push_str("avg_cache_duration_ns ");
    output.push_str(&f64::from_bits(AVG_CACHE_DURATION.load(Ordering::Relaxed)).to_string());
    output.push('\n');
    
    output.push_str("# HELP avg_proxy_duration_ns Average proxy duration in nanoseconds\n");
    output.push_str("# TYPE avg_proxy_duration_ns gauge\n");
    output.push_str("avg_proxy_duration_ns ");
    output.push_str(&f64::from_bits(AVG_PROXY_DURATION.load(Ordering::Relaxed)).to_string());
    output.push('\n');
    
    output.push_str("# HELP avg_error_duration_ns Average error duration in nanoseconds\n");
    output.push_str("# TYPE avg_error_duration_ns gauge\n");
    output.push_str("avg_error_duration_ns ");
    output.push_str(&f64::from_bits(AVG_ERROR_DURATION.load(Ordering::Relaxed)).to_string());
    output.push('\n');
    
    output.push_str("# HELP cpu_usage_cores CPU usage in cores (number of CPU cores utilized)\n");
    output.push_str("# TYPE cpu_usage_cores gauge\n");
    output.push_str("cpu_usage_cores ");
    output.push_str(&f64::from_bits(CPU_USAGE_CORES.load(Ordering::Relaxed)).to_string());
    output.push('\n');
    
    // Eviction metrics
    output.push_str("# HELP soft_evicted_total_items Total items evicted by soft eviction\n");
    output.push_str("# TYPE soft_evicted_total_items counter\n");
    output.push_str("soft_evicted_total_items ");
    output.push_str(&SOFT_EVICTIONS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP soft_evicted_total_bytes Total bytes evicted by soft eviction\n");
    output.push_str("# TYPE soft_evicted_total_bytes counter\n");
    output.push_str("soft_evicted_total_bytes ");
    output.push_str(&SOFT_BYTES_EVICTED.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP soft_evicted_total_scans Total scans for soft eviction\n");
    output.push_str("# TYPE soft_evicted_total_scans counter\n");
    output.push_str("soft_evicted_total_scans ");
    output.push_str(&SOFT_SCANS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP hard_evicted_total_items Total items evicted by hard eviction\n");
    output.push_str("# TYPE hard_evicted_total_items counter\n");
    output.push_str("hard_evicted_total_items ");
    output.push_str(&HARD_EVICTIONS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP hard_evicted_total_bytes Total bytes evicted by hard eviction\n");
    output.push_str("# TYPE hard_evicted_total_bytes counter\n");
    output.push_str("hard_evicted_total_bytes ");
    output.push_str(&HARD_BYTES_EVICTED.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP admission_allowed Total items allowed by admission control\n");
    output.push_str("# TYPE admission_allowed counter\n");
    output.push_str("admission_allowed ");
    output.push_str(&ADMISSION_ALLOWED.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP admission_not_allowed Total items not allowed by admission control\n");
    output.push_str("# TYPE admission_not_allowed counter\n");
    output.push_str("admission_not_allowed ");
    output.push_str(&ADMISSION_NOT_ALLOWED.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    // Refresh metrics
    output.push_str("# HELP refresh_updated Total items updated by refresh\n");
    output.push_str("# TYPE refresh_updated counter\n");
    output.push_str("refresh_updated ");
    output.push_str(&REFRESH_UPDATED.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP refresh_errors Total refresh errors\n");
    output.push_str("# TYPE refresh_errors counter\n");
    output.push_str("refresh_errors ");
    output.push_str(&REFRESH_ERRORS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP refresh_scans Total refresh scans\n");
    output.push_str("# TYPE refresh_scans counter\n");
    output.push_str("refresh_scans ");
    output.push_str(&REFRESH_SCANS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP refresh_hits Total refresh hits\n");
    output.push_str("# TYPE refresh_hits counter\n");
    output.push_str("refresh_hits ");
    output.push_str(&REFRESH_HITS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    output.push_str("# HELP refresh_miss Total refresh misses\n");
    output.push_str("# TYPE refresh_miss counter\n");
    output.push_str("refresh_miss ");
    output.push_str(&REFRESH_MISS.load(Ordering::Relaxed).to_string());
    output.push('\n');
    
    // Status code counters with labels
    output.push_str("# HELP resp_status_total Total number of HTTP responses by status code\n");
    output.push_str("# TYPE resp_status_total counter\n");
    let counters = get_status_code_counters();
    for (code, counter) in counters.iter().enumerate() {
        let count = counter.load(Ordering::Relaxed);
        if count > 0 {
            output.push_str("resp_status_total{code=\"");
            output.push_str(&code.to_string());
            output.push_str("\"} ");
            output.push_str(&count.to_string());
            output.push('\n');
        }
    }
    
    output
}

/// PrometheusMetricsController handles Prometheus metrics endpoint.
pub struct PrometheusMetricsController;

impl PrometheusMetricsController {
    /// Creates a new Prometheus metrics controller.
    pub fn new() -> Self {
        Self
    }

    /// Handles the metrics request.
    async fn get_metrics() -> impl IntoResponse {
        let metrics_text = format_prometheus_metrics();
        
        (
            StatusCode::OK,
            [("content-type", "text/plain; charset=utf-8")],
            metrics_text,
        )
    }
}

impl Default for PrometheusMetricsController {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller for PrometheusMetricsController {
    fn add_route(&self, router: Router) -> Router {
        router.route(PROMETHEUS_METRICS_PATH, get(Self::get_metrics))
    }
}
