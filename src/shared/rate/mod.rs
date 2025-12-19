//! Rate limiting functionality.
//

use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Rate limiter with token bucket.
pub struct Limiter {
    ch: mpsc::Receiver<()>,
}

impl Limiter {
    /// Creates a new rate limiter.
    pub fn new(shutdown_token: CancellationToken, limit: usize) -> Self {
        let burst = (limit as f64 * 0.1) as usize;
        let burst = burst.max(1);

        let (tx, rx) = mpsc::channel(burst);

        // Create rate limiter
        let quota = Quota::per_second(NonZeroU32::new(limit as u32).unwrap());
        let limiter = Arc::new(RateLimiter::direct(quota));

        // Spawn provider task
        let limiter_clone = limiter.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(1) / limit as u32);
            loop {
                tokio::select! {
                    _ = shutdown_token.cancelled() => {
                        break;
                    }
                    _ = interval.tick() => {
                        limiter_clone.check().ok();
                        if tx.send(()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Self { ch: rx }
    }

    /// Takes a token from the limiter (blocks until available).
    pub async fn take(&mut self) {
        let _ = self.ch.recv().await;
    }
}
