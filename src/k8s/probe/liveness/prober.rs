// Package liveness provides the Prober trait for liveness checking.

use super::Service;

/// Prober can handle services/applications.
pub trait Prober: Send + Sync {
    /// Watches services for liveness.
    fn watch(&mut self, services: Vec<Box<dyn Service>>);

    /// Checks whether the target service is alive (synchronous version).
    /// 
    /// This is a blocking call that internally uses async runtime.
    fn is_alive(&mut self) -> bool;
}

