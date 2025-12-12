// Package workers provides worker configuration.

use std::sync::Arc;
use std::time::Duration;

use crate::governor::{Config, Freq};

/// CallFreq represents call frequency configuration.
#[derive(Debug, Clone)]
pub struct CallFreq {
    rate_limit: usize,
    tick_freq: Duration,
}

impl CallFreq {
    /// Creates a new CallFreq.
    pub fn new(rate_limit: usize, tick_freq: Duration) -> Self {
        Self {
            rate_limit,
            tick_freq,
        }
    }
}

impl Freq for CallFreq {
    fn is_rate_limit_defined(&self) -> bool {
        self.rate_limit > 0
    }

    fn is_tick_freq_defined(&self) -> bool {
        !self.tick_freq.is_zero()
    }

    fn get_rate_limit(&self) -> usize {
        self.rate_limit
    }

    fn set_rate_limit(&self, new: usize) -> Arc<dyn Freq> {
        Arc::new(Self {
            rate_limit: new,
            tick_freq: self.tick_freq,
        })
    }

    fn get_tick_freq(&self) -> Duration {
        self.tick_freq
    }

    fn set_tick_freq(&self, new: Duration) -> Arc<dyn Freq> {
        Arc::new(Self {
            rate_limit: self.rate_limit,
            tick_freq: new,
        })
    }

    fn clone_freq(&self) -> Arc<dyn Freq> {
        Arc::new(self.clone())
    }
}

/// Config represents worker configuration.
#[derive(Clone)]
pub struct WorkerConfig {
    enabled: bool,
    replicas: usize,
    freq: Arc<dyn Freq>,
}

impl WorkerConfig {
    /// Creates a new Config.
    pub fn new(enabled: bool, freq: Arc<dyn Freq>, replicas: usize) -> Self {
        Self {
            enabled,
            replicas,
            freq,
        }
    }
}

impl Config for WorkerConfig {
    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&self, v: bool) -> Arc<dyn Config> {
        Arc::new(Self {
            enabled: v,
            replicas: self.replicas,
            freq: self.freq.clone_freq(),
        })
    }

    fn get_replicas(&self) -> usize {
        self.replicas
    }

    fn set_replicas(&self, n: usize) -> Arc<dyn Config> {
        Arc::new(Self {
            enabled: self.enabled,
            replicas: n,
            freq: self.freq.clone_freq(),
        })
    }

    fn get_freq(&self) -> Arc<dyn Freq> {
        self.freq.clone_freq()
    }

    fn set_freq(&self, freq: Arc<dyn Freq>) -> Arc<dyn Config> {
        Arc::new(Self {
            enabled: self.enabled,
            replicas: self.replicas,
            freq,
        })
    }
}

