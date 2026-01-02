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
fn format_prometheus_metrics() -> String {
    let mut output = String::new();
    
    // Counters
    output.push_str(&format!("# HELP cache_hits Total number of cache hits\n"));
    output.push_str(&format!("# TYPE cache_hits counter\n"));
    output.push_str(&format!("cache_hits {}\n", CACHE_HITS.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP cache_misses Total number of cache misses\n"));
    output.push_str(&format!("# TYPE cache_misses counter\n"));
    output.push_str(&format!("cache_misses {}\n", CACHE_MISSES.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP total Total number of requests\n"));
    output.push_str(&format!("# TYPE total counter\n"));
    output.push_str(&format!("total {}\n", TOTAL_REQUESTS.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP errors Total number of errors\n"));
    output.push_str(&format!("# TYPE errors counter\n"));
    output.push_str(&format!("errors {}\n", ERRORED_REQUESTS.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP proxies Total number of proxied requests\n"));
    output.push_str(&format!("# TYPE proxies counter\n"));
    output.push_str(&format!("proxies {}\n", PROXIED_REQUESTS.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP panics Total number of panics\n"));
    output.push_str(&format!("# TYPE panics counter\n"));
    output.push_str(&format!("panics {}\n", PANICKED_REQUESTS.load(Ordering::Relaxed)));
    
    // Gauges
    output.push_str(&format!("# HELP rps Requests per second\n"));
    output.push_str(&format!("# TYPE rps gauge\n"));
    output.push_str(&format!("rps {}\n", f64::from_bits(RPS.load(Ordering::Relaxed))));
    
    output.push_str(&format!("# HELP cache_memory_usage Cache logical memory usage in bytes (memory used by cache data structures)\n"));
    output.push_str(&format!("# TYPE cache_memory_usage gauge\n"));
    output.push_str(&format!("cache_memory_usage {}\n", CACHE_MEMORY_USAGE.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP process_physical_memory_usage Process physical memory usage in bytes (RSS - Resident Set Size from system)\n"));
    output.push_str(&format!("# TYPE process_physical_memory_usage gauge\n"));
    output.push_str(&format!("process_physical_memory_usage {}\n", PROCESS_PHYSICAL_MEMORY_USAGE.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP cache_length Number of items in cache\n"));
    output.push_str(&format!("# TYPE cache_length gauge\n"));
    output.push_str(&format!("cache_length {}\n", CACHE_LENGTH.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP avg_duration_ns Average total duration in nanoseconds\n"));
    output.push_str(&format!("# TYPE avg_duration_ns gauge\n"));
    output.push_str(&format!("avg_duration_ns {}\n", f64::from_bits(AVG_TOTAL_DURATION.load(Ordering::Relaxed))));
    
    output.push_str(&format!("# HELP avg_cache_duration_ns Average cache duration in nanoseconds\n"));
    output.push_str(&format!("# TYPE avg_cache_duration_ns gauge\n"));
    output.push_str(&format!("avg_cache_duration_ns {}\n", f64::from_bits(AVG_CACHE_DURATION.load(Ordering::Relaxed))));
    
    output.push_str(&format!("# HELP avg_proxy_duration_ns Average proxy duration in nanoseconds\n"));
    output.push_str(&format!("# TYPE avg_proxy_duration_ns gauge\n"));
    output.push_str(&format!("avg_proxy_duration_ns {}\n", f64::from_bits(AVG_PROXY_DURATION.load(Ordering::Relaxed))));
    
    output.push_str(&format!("# HELP avg_error_duration_ns Average error duration in nanoseconds\n"));
    output.push_str(&format!("# TYPE avg_error_duration_ns gauge\n"));
    output.push_str(&format!("avg_error_duration_ns {}\n", f64::from_bits(AVG_ERROR_DURATION.load(Ordering::Relaxed))));
    
    output.push_str(&format!("# HELP cpu_usage_cores CPU usage in cores (number of CPU cores utilized)\n"));
    output.push_str(&format!("# TYPE cpu_usage_cores gauge\n"));
    output.push_str(&format!("cpu_usage_cores {}\n", f64::from_bits(CPU_USAGE_CORES.load(Ordering::Relaxed))));
    
    // Eviction metrics
    output.push_str(&format!("# HELP soft_evicted_total_items Total items evicted by soft eviction\n"));
    output.push_str(&format!("# TYPE soft_evicted_total_items counter\n"));
    output.push_str(&format!("soft_evicted_total_items {}\n", SOFT_EVICTIONS.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP soft_evicted_total_bytes Total bytes evicted by soft eviction\n"));
    output.push_str(&format!("# TYPE soft_evicted_total_bytes counter\n"));
    output.push_str(&format!("soft_evicted_total_bytes {}\n", SOFT_BYTES_EVICTED.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP soft_evicted_total_scans Total scans for soft eviction\n"));
    output.push_str(&format!("# TYPE soft_evicted_total_scans counter\n"));
    output.push_str(&format!("soft_evicted_total_scans {}\n", SOFT_SCANS.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP hard_evicted_total_items Total items evicted by hard eviction\n"));
    output.push_str(&format!("# TYPE hard_evicted_total_items counter\n"));
    output.push_str(&format!("hard_evicted_total_items {}\n", HARD_EVICTIONS.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP hard_evicted_total_bytes Total bytes evicted by hard eviction\n"));
    output.push_str(&format!("# TYPE hard_evicted_total_bytes counter\n"));
    output.push_str(&format!("hard_evicted_total_bytes {}\n", HARD_BYTES_EVICTED.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP admission_allowed Total items allowed by admission control\n"));
    output.push_str(&format!("# TYPE admission_allowed counter\n"));
    output.push_str(&format!("admission_allowed {}\n", ADMISSION_ALLOWED.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP admission_not_allowed Total items not allowed by admission control\n"));
    output.push_str(&format!("# TYPE admission_not_allowed counter\n"));
    output.push_str(&format!("admission_not_allowed {}\n", ADMISSION_NOT_ALLOWED.load(Ordering::Relaxed)));
    
    // Refresh metrics
    output.push_str(&format!("# HELP refresh_updated Total items updated by refresh\n"));
    output.push_str(&format!("# TYPE refresh_updated counter\n"));
    output.push_str(&format!("refresh_updated {}\n", REFRESH_UPDATED.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP refresh_errors Total refresh errors\n"));
    output.push_str(&format!("# TYPE refresh_errors counter\n"));
    output.push_str(&format!("refresh_errors {}\n", REFRESH_ERRORS.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP refresh_scans Total refresh scans\n"));
    output.push_str(&format!("# TYPE refresh_scans counter\n"));
    output.push_str(&format!("refresh_scans {}\n", REFRESH_SCANS.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP refresh_hits Total refresh hits\n"));
    output.push_str(&format!("# TYPE refresh_hits counter\n"));
    output.push_str(&format!("refresh_hits {}\n", REFRESH_HITS.load(Ordering::Relaxed)));
    
    output.push_str(&format!("# HELP refresh_miss Total refresh misses\n"));
    output.push_str(&format!("# TYPE refresh_miss counter\n"));
    output.push_str(&format!("refresh_miss {}\n", REFRESH_MISS.load(Ordering::Relaxed)));
    
    // Status code counters with labels
    output.push_str(&format!("# HELP resp_status_total Total number of HTTP responses by status code\n"));
    output.push_str(&format!("# TYPE resp_status_total counter\n"));
    let counters = get_status_code_counters();
    for (code, counter) in counters.iter().enumerate() {
        let count = counter.load(Ordering::Relaxed);
        if count > 0 {
            output.push_str(&format!("resp_status_total{{code=\"{}\"}} {}\n", code, count));
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
