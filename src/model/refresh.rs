use std::sync::atomic::Ordering;
use std::f64;

use crate::time;
use crate::config::{Config, ConfigTrait};
use super::Entry;

impl Entry {
    /// Checks that elapsed time is greater than TTL (used in hotpath: GET).
    pub fn is_expired(&self, cfg: &Config) -> bool {
        let ttl = cfg.lifetime()
            .and_then(|l| l.ttl)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        
        // Time since the last successful refresh.
        let updated_at = self.updated_at.load(Ordering::Relaxed);
        let elapsed = time::unix_nano() - updated_at;
        
        elapsed > ttl
    }

    /// Implements probabilistic refresh logic ("beta" algorithm) and used while background refresh.
    /// Returns true if the entry is stale and, with a probability proportional to its staleness, should be refreshed now.
    pub fn is_probably_expired(&self, cfg: &Config) -> bool {
        let mut ttl = cfg.lifetime()
            .and_then(|l| l.ttl)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        let mut beta = cfg.lifetime()
            .and_then(|l| l.beta)
            .unwrap_or(1.0);
        let mut coefficient = cfg.lifetime()
            .and_then(|l| l.coefficient)
            .unwrap_or(0.0);

        // Per-entry overrides (if present).
        if let Some(ref lifetime) = self.rule.refresh {
            if !lifetime.enabled {
                return false;
            }
            if let Some(rule_ttl) = lifetime.ttl {
                let rule_ttl_nanos = rule_ttl.as_nanos() as i64;
                if rule_ttl_nanos > 0 {
                    ttl = rule_ttl_nanos;
                }
            }
            if let Some(rule_beta) = lifetime.beta {
                if rule_beta > 0.0 {
                    beta = rule_beta;
                }
            }
            if let Some(rule_coefficient) = lifetime.coefficient {
                if rule_coefficient > 0.0 {
                    coefficient = rule_coefficient;
                }
            }
        }

        // Time since the last successful refresh.
        let updated_at = self.updated_at.load(Ordering::Relaxed);
        let elapsed = time::unix_nano() - updated_at;
        
        // Hard floor: do nothing until elapsed >= coefficient * ttl.
        let min_stale = (ttl as f64 * coefficient) as i64;

        if min_stale > elapsed {
            return false;
        }

        // Normalize x = elapsed / ttl into [0,1].
        let mut x = elapsed as f64 / ttl as f64;
        if x < 0.0 {
            x = 0.0;
        } else if x > 1.0 {
            x = 1.0;
        }

        // Lifetime probability via the exponential CDF:
        // p = 1 - exp(-beta * x). Larger beta -> steeper growth.
        let probability = 1.0 - (-beta * x).exp();
        
        // Use thread_rng for random number generation (equivalent to rnd.Float64() in Go)
        use rand::Rng;
        let rnd_value = rand::thread_rng().gen_range(0.0..1.0);
        rnd_value < probability
    }

    /// Tries to mark the entry as refresh queued.
    pub fn try_mark_refresh_queued(&self) -> bool {
        self.refresh_queued
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    /// Clears the refresh queued flag.
    pub fn clear_refresh_queued(&self) {
        self.refresh_queued.store(false, Ordering::Relaxed);
    }
}

