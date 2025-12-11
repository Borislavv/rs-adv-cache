// Package shutdown provides graceful shutdown functionality.

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tokio::time::timeout;
use tracing::{error, info, warn};

#[derive(Debug, thiserror::Error)]
#[error("graceful shutdown timeout exceeded")]
pub struct TimeoutError;

/// Graceful shutdown handler
/// Similar to Go's Graceful struct but using Rust async primitives
#[derive(Clone)]
pub struct GracefulShutdown {
    shutdown_token: CancellationToken,
    timeout: Arc<tokio::sync::RwLock<Duration>>,
    counter: Arc<tokio::sync::Semaphore>,
}

impl GracefulShutdown {
    /// Creates a new graceful shutdown handler
    pub fn new(shutdown_token: CancellationToken) -> Self {
        Self {
            shutdown_token,
            timeout: Arc::new(tokio::sync::RwLock::new(Duration::from_secs(10))),
            counter: Arc::new(tokio::sync::Semaphore::new(0)),
        }
    }

    /// Sets the graceful shutdown timeout
    pub async fn set_graceful_timeout(&self, timeout: Duration) {
        *self.timeout.write().await = timeout;
    }

    /// Adds to the wait counter (similar to sync.WaitGroup.Add)
    pub fn add(&self, n: usize) {
        // Add permits to the semaphore
        self.counter.add_permits(n);
    }

    /// Marks one task as done (similar to sync.WaitGroup.Done)
    pub fn done(&self) {
        // Release one permit
        let _ = self.counter.try_acquire();
    }

    /// Waits for shutdown signal and then waits for all tasks to complete
    pub async fn await_shutdown(&self) -> Result<()> {
        // Wait for either OS signal or cancellation
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!(
                    component = "graceful-shutdown",
                    event = "os_signal",
                    signal = "SIGINT",
                    "cancellation started"
                );
            }
            _ = self.shutdown_token.cancelled() => {
                info!(
                    component = "graceful-shutdown",
                    event = "ctx_done",
                    "cancellation started"
                );
            }
        }

        self.cancel_and_await_with_timeout().await
    }

    async fn cancel_and_await_with_timeout(&self) -> Result<()> {
        // Cancel the shutdown token
        self.shutdown_token.cancel();

        let timeout_duration = *self.timeout.read().await;
        
        // Wait for all tasks to complete, with timeout
        match timeout(timeout_duration, self.wait_for_completion()).await {
            Ok(_) => {
                info!(
                    component = "graceful-shutdown",
                    event = "shutdown_success",
                    "service was gracefully shut down"
                );
                Ok(())
            }
            Err(_) => {
                warn!(
                    component = "graceful-shutdown",
                    event = "shutdown_timeout",
                    timeout_secs = timeout_duration.as_secs(),
                    "not all tasks were closed within timeout"
                );
                Err(TimeoutError.into())
            }
        }
    }

    async fn wait_for_completion(&self) {
        // Wait until all permits are acquired (all tasks are done)
        // The semaphore starts with 0 permits, and we add permits with add()
        // Each task calls done() which tries to acquire a permit
        // When all permits are acquired, all tasks are done
        let initial_permits = self.counter.available_permits();
        for _ in 0..initial_permits {
            let _permit = self.counter.acquire().await;
        }
    }
}

