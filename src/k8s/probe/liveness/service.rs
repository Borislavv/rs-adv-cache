// Service trait for liveness checking

use std::time::Duration;

/// Service interface for liveness checking
pub trait Service: Send + Sync {
    /// Checks if the service is alive
    fn is_alive(&self, timeout: Duration) -> bool;
}
