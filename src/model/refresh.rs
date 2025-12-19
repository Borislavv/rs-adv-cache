use std::sync::atomic::Ordering;

use super::Entry;
use crate::config::{Config, ConfigTrait};
use crate::rand;
use crate::time;

impl Entry {
    /// Checks that elapsed time is greater than TTL (used in hotpath: GET).
    pub fn is_expired(&self, cfg: &Config) -> bool {
        let ttl = cfg
            .lifetime()
            .and_then(|l| l.ttl)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);

        let updated_at = self.0.updated_at.load(Ordering::Relaxed);
        let elapsed = time::unix_nano() - updated_at;

        elapsed > ttl
    }

    /// Implements probabilistic refresh logic (beta algorithm) for background refresh.
    /// Returns true if the entry is stale and, with a probability proportional to its staleness, should be refreshed now.
    pub fn is_probably_expired(&self, cfg: &Config) -> bool {
        let lifetime = cfg.lifetime();
        let mut ttl = lifetime
            .and_then(|l| l.ttl)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        let mut beta = lifetime.and_then(|l| l.beta).unwrap_or(1.0);
        let mut coefficient = lifetime.and_then(|l| l.coefficient).unwrap_or(0.0);

        // Per-entry overrides (if present).
        if let Some(rule_lifetime) = &self.0.rule.refresh {
            if !rule_lifetime.enabled {
                return false;
            }
            if let Some(ref ttl_duration) = rule_lifetime.ttl {
                if ttl_duration.as_nanos() > 0 {
                    ttl = ttl_duration.as_nanos() as i64;
                }
            }
            if let Some(beta_val) = rule_lifetime.beta {
                if beta_val > 0.0 {
                    beta = beta_val;
                }
            }
            if let Some(coeff_val) = rule_lifetime.coefficient {
                coefficient = coeff_val;
            }
        }

        let updated_at = self.0.updated_at.load(Ordering::Relaxed);
        let elapsed = time::unix_nano() - updated_at;
        let min_stale = ((ttl as f64) * coefficient).round() as i64;

        if elapsed < min_stale {
            return false;
        }

        let x = (elapsed as f64 / ttl as f64).clamp(0.0, 1.0);

        // Lifetime probability via the exponential CDF:
        // p = 1 - exp(-beta * x). Larger beta -> steeper growth.
        let probability = 1.0 - (-beta * x).exp();
        rand::float64() < probability
    }

    /// Tries to mark the entry as refresh queued.
    pub fn try_mark_refresh_queued(&self) -> bool {
        self.0.refresh_queued
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    /// Clears the refresh queued flag.
    pub fn clear_refresh_queued(&self) {
        self.0.refresh_queued.store(false, Ordering::Relaxed);
    }
}
