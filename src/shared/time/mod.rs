//! Cached time to avoid syscalls.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;

static NOW_UNIX: AtomicI64 = AtomicI64::new(0);

/// Starts the time caching ticker.
/// Updates the cached time value at the specified resolution.
/// Returns a function that can be called to stop the ticker.
pub fn start(resolution: Duration) -> CancellationToken {
    // Initialize with current time
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64;
    NOW_UNIX.store(now, Ordering::Relaxed);

    // Create cancellation token for stopping the ticker
    let token = CancellationToken::new();
    let token_clone = token.clone();

    // Spawn task to update time periodically
    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(resolution);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as i64;
                    NOW_UNIX.store(now, Ordering::Relaxed);
                }
                _ = token_clone.cancelled() => {
                    break;
                }
            }
        }
    });

    token
}

/// Returns the cached current time.
pub fn now() -> SystemTime {
    let nanos = NOW_UNIX.load(Ordering::Relaxed);
    UNIX_EPOCH + Duration::from_nanos(nanos as u64)
}

/// Returns the cached current time as Unix nanoseconds.
pub fn unix_nano() -> i64 {
    NOW_UNIX.load(Ordering::Relaxed)
}

/// Returns the duration elapsed since the given time.
pub fn since(t: SystemTime) -> Duration {
    now().duration_since(t).unwrap_or(Duration::ZERO)
}
