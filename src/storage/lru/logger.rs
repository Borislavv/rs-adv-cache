// Package lru provides logging for LRU storage.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tokio::time::interval;

use crate::bytes;
use crate::config::{Config, ConfigTrait};
use crate::metrics;

// Global admission counters.
lazy_static::lazy_static! {
    pub static ref ADMISSION_ALLOWED: Arc<AtomicI64> = Arc::new(AtomicI64::new(0));
    pub static ref ADMISSION_NOT_ALLOWED: Arc<AtomicI64> = Arc::new(AtomicI64::new(0));
    pub static ref EVICTED_HARD_LIMIT_ITEMS: Arc<AtomicI64> = Arc::new(AtomicI64::new(0));
    pub static ref EVICTED_HARD_LIMIT_BYTES: Arc<AtomicI64> = Arc::new(AtomicI64::new(0));
}

/// Logger for LRU storage metrics.
pub async fn logger(
    shutdown_token: CancellationToken,
    cfg: Arc<tokio::sync::RwLock<Config>>,
    soft_memory_limit: i64,
    hard_memory_limit: i64,
    mem: Arc<dyn Fn() -> i64 + Send + Sync>,
    len: Arc<dyn Fn() -> i64 + Send + Sync>,
) {
    let mut each_sec = interval(Duration::from_secs(1));
    let mut each_5sec = interval(Duration::from_secs(5));

    let soft_limit = bytes::fmt_mem(soft_memory_limit);
    let hard_limit = bytes::fmt_mem(hard_memory_limit);

    let mut adm_allowed_5s = 0i64;
    let mut adm_not_allowed_5s = 0i64;
    let mut hard_evicted_5s = 0i64;
    let mut hard_evicted_bytes_5s = 0i64;

    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                return;
            }
            _ = each_sec.tick() => {
                let hard_evicted_1s = EVICTED_HARD_LIMIT_ITEMS.swap(0, Ordering::Relaxed);
                let hard_evicted_bytes_1s = EVICTED_HARD_LIMIT_BYTES.swap(0, Ordering::Relaxed);
                hard_evicted_5s += hard_evicted_1s;
                hard_evicted_bytes_5s += hard_evicted_bytes_1s;

                let adm_allowed_1s = ADMISSION_ALLOWED.swap(0, Ordering::Relaxed);
                let adm_not_allowed_1s = ADMISSION_NOT_ALLOWED.swap(0, Ordering::Relaxed);
                adm_allowed_5s += adm_allowed_1s;
                adm_not_allowed_5s += adm_not_allowed_1s;

                metrics::add_hard_eviction_stat_counters(
                    hard_evicted_1s,
                    hard_evicted_bytes_1s,
                    adm_allowed_1s,
                    adm_not_allowed_1s,
                );
            }
            _ = each_5sec.tick() => {
                let cfg_guard = cfg.read().await;
                let active = (*cfg_guard).admission()
                    .map(|a| a.is_enabled.load(Ordering::Relaxed))
                    .unwrap_or(false);

                tracing::info!(
                    active = %active,
                    allowed = adm_allowed_5s,
                    not_allowed = adm_not_allowed_5s,
                    "admission-control"
                );

                adm_allowed_5s = 0;
                adm_not_allowed_5s = 0;

                let freed_bytes_str = bytes::fmt_mem(hard_evicted_bytes_5s);
                tracing::info!(
                    freed_bytes = %freed_bytes_str,
                    freed_items = hard_evicted_5s,
                    soft_mem_limit = %soft_limit,
                    hard_mem_limit = %hard_limit,
                    "hard-eviction"
                );

                hard_evicted_5s = 0;
                hard_evicted_bytes_5s = 0;

                let usage = bytes::fmt_mem(mem());
                let length = len().to_string();
                let mode = (*cfg_guard).storage().mode.as_deref().unwrap_or("sampling");

                tracing::info!(
                    target = "storage",
                    mode = %mode,
                    usage = %usage,
                    soft_mem_limit = %soft_limit,
                    hard_mem_limit = %hard_limit,
                    entries = %length,
                    "storage stats"
                );
            }
        }
    }
}

