// Package upstream provides health probe functionality.

use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tokio::time::interval;

/// Health probe configuration constants.
#[allow(dead_code)]
const OK_CLOSE: usize = 3; // after 3 in a row -> UP
#[allow(dead_code)]
const FAIL_OPEN: usize = 3; // after 3 in a row -> DOWN
#[allow(dead_code)]
const BASE_PROBE: Duration = Duration::from_millis(500);
#[allow(dead_code)]
const DOWN_PROBE: Duration = Duration::from_secs(1); // on DOWN throttling probes

/// Observer runs health checks and updates backend health status.
#[allow(dead_code)]
pub async fn observer<F>(
    shutdown_token: CancellationToken,
    is_healthy: F,
    set_health: Arc<dyn Fn(bool) + Send + Sync>,
) where
    F: Fn() -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + Sync + 'static,
{
    let mut probe_interval = interval(BASE_PROBE);
    let mut down = false;
    let mut fails = 0;
    let mut oks = 0;

    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                return;
            }
            _ = probe_interval.tick() => {
                match is_healthy() {
                    Err(_) => {
                        // Fail
                        oks = 0;
                        if !down {
                            fails += 1;
                            if fails >= FAIL_OPEN {
                                down = true;
                                set_health(false);
                                fails = 0;
                                probe_interval = interval(DOWN_PROBE);
                            }
                        }
                    }
                    Ok(_) => {
                        // Success
                        fails = 0;
                        if down {
                            oks += 1;
                            if oks >= OK_CLOSE {
                                down = false;
                                set_health(true);
                                oks = 0;
                                probe_interval = interval(BASE_PROBE);
                            }
                        } else {
                            oks = 0;
                        }
                    }
                }
            }
        }
    }
}

