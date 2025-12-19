// Liveness probe functionality.
//

use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time::timeout;
use tracing::{error, warn};

pub mod error;
pub mod prober;
pub mod service;

pub use error::TimeoutIsTooShortError;
pub use prober::Prober;
pub use service::Service;

/// Liveness probe implementation
pub struct Probe {
    services: Arc<RwLock<Vec<Arc<dyn Service>>>>,
    timeout: Duration,
}

impl Probe {
    /// Creates a new liveness probe
    pub fn new(timeout_duration: Duration) -> Self {
        const MIN_TIMEOUT: Duration = Duration::from_millis(1);
        if timeout_duration < MIN_TIMEOUT {
            warn!(
                error = %TimeoutIsTooShortError,
                "min timeout duration is 1ms (timeout set up as 10ms as a more reasonable value)"
            );
            let _timeout_duration = Duration::from_millis(10);
        }

        Self {
            services: Arc::new(RwLock::new(Vec::new())),
            timeout: timeout_duration,
        }
    }

    /// Registers services to be checked for liveness.
    pub fn watch(&self, services: Vec<Arc<dyn Service>>) {
        let mut guard = self.services.write().expect("poisoned liveness lock");
        guard.extend(services);
    }

    /// Checks whether all watched services are alive (async).
    /// Checks if the service is alive (async version)
    pub async fn is_alive_async(&self) -> bool {
        let probe_timeout = self.timeout;

        match timeout(probe_timeout, async { self.check_services() }).await {
            Ok(result) => result,
            Err(_) => {
                warn!("liveness probe deadline exceeded while checking service");
                false
            }
        }
    }

    fn check_services(&self) -> bool {
        let services = {
            let guard = self.services.read().expect("poisoned liveness lock");
            guard.clone()
        };

        if services.is_empty() {
            return true;
        }

        for service in services {
            if !service.is_alive(self.timeout) {
                return false;
            }
        }
        true
    }
}

impl Service for Arc<Probe> {
    fn is_alive(&self, _timeout: Duration) -> bool {
        // Use the probe's internal timeout
        let handle = tokio::runtime::Handle::current();
        std::thread::scope(|scope| {
            scope
                .spawn(|| handle.block_on(async { self.is_alive_async().await }))
                .join()
                .unwrap_or_else(|_| {
                    error!("liveness probe thread panicked");
                    false
                })
        })
    }
}

impl Prober for Probe {
    fn watch(&self, services: Vec<Arc<dyn Service>>) {
        Probe::watch(self, services)
    }

    fn is_alive(&self) -> bool {
        self.check_services()
    }
}
