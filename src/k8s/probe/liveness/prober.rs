// Liveness prober trait.
//

use std::sync::Arc;

use super::Service;

/// Prober can handle services/applications.
pub trait Prober: Send + Sync {
    /// Registers services to be checked.
    fn watch(&self, services: Vec<Arc<dyn Service>>);

    /// Checks whether the target service is alive (synchronous version).
    fn is_alive(&self) -> bool;
}
