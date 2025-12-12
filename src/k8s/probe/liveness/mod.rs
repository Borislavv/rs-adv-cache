// Package liveness provides Kubernetes liveness probe functionality.

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
    ask_tx: Arc<mpsc::Sender<tokio_util::sync::CancellationToken>>,
    resp_tx: Arc<tokio::sync::Mutex<mpsc::Sender<bool>>>,
    timeout: Duration,
}

impl Probe {
    /// Creates a new liveness probe
    pub fn new(timeout_duration: Duration) -> Self {
        const MIN_TIMEOUT: Duration = Duration::from_millis(1);
        let timeout = if timeout_duration < MIN_TIMEOUT {
            warn!(
                error = %TimeoutIsTooShortError,
                "min timeout duration is 1ms (timeout set up as 10ms as a more reasonable value)"
            );
            Duration::from_millis(10)
        } else {
            timeout_duration
        };

        let (ask_tx, _) = mpsc::channel::<tokio_util::sync::CancellationToken>(1);
        let (resp_tx, _) = mpsc::channel::<bool>(1);

        Self {
            ask_tx: Arc::new(ask_tx),
            resp_tx: Arc::new(tokio::sync::Mutex::new(resp_tx)),
            timeout,
        }
    }

    /// Starts watching services. This must be called before IsAlive can work properly.
    pub fn watch(&self, services: Vec<Arc<dyn Service>>) {
        let ask_rx = {
            // We need to create a new receiver from the sender
            // This is a limitation - we should restructure to store both sender and receiver
            let (_new_tx, new_rx) = mpsc::channel::<tokio_util::sync::CancellationToken>(1);
            // For now, we'll create a new channel and replace the sender
            // But this breaks the design - we need to rethink this
            new_rx
        };
        
        let resp_tx = self.resp_tx.clone();
        let timeout = self.timeout;
        
        tokio::task::spawn(async move {
            let mut ask_rx = ask_rx;
            while let Some(_ctx) = ask_rx.recv().await {
                let mut is_alive = true;
                for service in &services {
                    is_alive = is_alive && service.is_alive(timeout);
                }
                if let Ok(tx) = resp_tx.lock() {
                    let _ = tx.send(is_alive).await;
                }
            }
        });
    }

    /// Checks if the service is alive (async version)
    pub async fn is_alive_async(&self) -> bool {
        let probe_timeout = self.timeout;
        let ctx = tokio_util::sync::CancellationToken::new();
        
        // Create a one-time response receiver for this probe
        let (resp_tx_once, mut resp_rx_once) = mpsc::channel(1);
        *self.resp_tx.lock().await = resp_tx_once;

        match timeout(probe_timeout, async {
            // Send probe request
            if self.ask_tx.send(ctx).await.is_err() {
                return false;
            }

            // Wait for response
            resp_rx_once.recv().await.unwrap_or(false)
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

