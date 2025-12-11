// Package liveness provides Kubernetes liveness probe functionality.
//

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::warn;

pub mod error;
pub mod service;
pub mod config;
pub mod prober;

pub use error::TimeoutIsTooShortError;
pub use service::Service;

/// Liveness probe implementation
pub struct Probe {
    ask_tx: mpsc::Sender<Duration>,
    resp_rx: tokio::sync::Mutex<mpsc::Receiver<bool>>,
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

        let (ask_tx, ask_rx) = mpsc::channel(1);
        let (resp_tx, resp_rx) = mpsc::channel(1);

        // Spawn the watch task
        tokio::task::spawn(async move {
            let mut ask_rx = ask_rx;
            while let Some(_probe_timeout) = ask_rx.recv().await {
                // For now, just return true (service is alive)
                // In the future, this could check watched services
                let _ = resp_tx.send(true).await;
            }
        });

        Self {
            ask_tx,
            resp_rx: tokio::sync::Mutex::new(resp_rx),
            timeout: timeout_duration,
        }
    }

    /// Checks if the service is alive (async version)
    pub async fn is_alive_async(&self) -> bool {
        let probe_timeout = self.timeout;

        match timeout(probe_timeout, async {
            // Send probe request
            if self.ask_tx.send(probe_timeout).await.is_err() {
                return false;
            }

            // Wait for response
            let mut rx = self.resp_rx.lock().await;
            rx.recv().await.unwrap_or(false)
        }).await {
            Ok(result) => result,
            Err(_) => {
                warn!(
                    "liveness probe deadline exceeded while checking service"
                );
                false
            }
        }
    }
}

impl Service for Arc<Probe> {
    fn is_alive(&self, _timeout: Duration) -> bool {
        // Use the probe's internal timeout
        // Use std::thread::scope to avoid blocking the runtime thread
        let handle = tokio::runtime::Handle::current();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                handle.block_on(async {
                    self.is_alive_async().await
                })
            }).join().unwrap()
        })
    }
}

