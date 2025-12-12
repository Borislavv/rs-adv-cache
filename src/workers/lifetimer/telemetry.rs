// Package lifetimer provides telemetry for lifetime management.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tokio::time::interval;

use crate::config::{Config as AppConfig, ConfigTrait};
use crate::metrics;
use crate::governor::Config;

use super::counters::Counters;

/// Telemetry logger for lifetime management.
pub async fn logger(
    shutdown_token: CancellationToken,
    name: String,
    counters: Arc<Counters>,
    cfg: Arc<tokio::sync::RwLock<Arc<dyn Config>>>,
    g_cfg: AppConfig,
    w_num_active: Arc<std::sync::atomic::AtomicI64>,
    each: Duration,
) {
    let mut ticker = interval(each);

    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                tracing::debug!(svc = "lifetimer", name = %name, "logger stopped");
                return;
            }
            _ = ticker.tick() => {
                let workers = w_num_active.load(Ordering::Relaxed);
                let active = cfg.read().await.as_ref().is_enabled();
                let (affected, errors, scans, miss, hits) = counters.reset();
                
                metrics::add_lifetime_stat_counters(affected, errors, scans, miss, hits);

                let on_ttl = if g_cfg.lifetime().map(|l| l.is_remove_on_ttl.load(Ordering::Relaxed)).unwrap_or(false) {
                    "remove"
                } else {
                    "refresh"
                };

                tracing::info!(
                    on_ttl = %on_ttl,
                    active = %active,
                    replicas = workers,
                    errors = errors,
                    affected = affected,
                    scans = scans,
                    scans_hit = hits,
                    scans_miss = miss,
                    name = %name,
                    "lifetime manager stats"
                );
            }
        }
    }
}

/// Gets a stats snapshot.
#[allow(dead_code)]
pub fn stats_snapshot(
    name: &str,
    w_num_active: &std::sync::atomic::AtomicI64,
    is_enabled: bool,
) -> (String, i64, bool) {
    (name.to_string(), w_num_active.load(Ordering::Relaxed), is_enabled)
}

