// Package lfu provides admission control interface.

use crate::config::Admission as AdmissionConfig;

use super::tiny_lfu::ShardedAdmitter;

/// Admission control interface.
pub trait Admission: Send + Sync {
    /// Records a key access.
    fn record(&self, h: u64);

    /// Returns true if the candidate should replace a victim.
    fn allow(&self, candidate: u64, victim: u64) -> bool;

    /// Exposes frequency estimate (for metrics/diagnostics).
    #[allow(dead_code)]
    fn estimate(&self, h: u64) -> u8;
}

impl Admission for ShardedAdmitter {
    fn record(&self, h: u64) {
        ShardedAdmitter::record(self, h);
    }

    fn allow(&self, candidate: u64, victim: u64) -> bool {
        ShardedAdmitter::allow(self, candidate, victim)
    }
}

/// Creates a new admission controller.
pub fn new_admission(cfg: Option<&AdmissionConfig>) -> Box<dyn Admission> {
    let cfg = match cfg {
        Some(c) => c,
        None => return Box::new(ShardedAdmitter::default()),
    };
    Box::new(ShardedAdmitter::new(cfg))
}

