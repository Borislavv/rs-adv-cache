// Package orchestrator provides the Governor interface.

use anyhow::Result;

use super::service::{Config, Service};
use std::sync::Arc;

/// Governor interface for orchestrating services.
pub trait Governor: Send + Sync {
    /// Registers a service with the orchestrator.
    fn register(&self, name: String, s: Arc<dyn Service>);

    /// Gets the configuration for a service.
    fn cfg(&self, name: &str) -> Result<Arc<dyn Config>>;

    /// Turns on a service.
    fn on(&self, name: &str) -> Result<()>;

    /// Turns off a service.
    fn off(&self, name: &str) -> Result<()>;

    /// Starts a service.
    fn start(&self, name: &str) -> Result<()>;

    /// Reloads a service with new configuration.
    fn reload(&self, name: &str, cfg: Arc<dyn Config>) -> Result<()>;

    /// Scales a service to n replicas.
    fn scale_to(&self, name: &str, n: usize) -> Result<()>;
}

