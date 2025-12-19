use dashmap::DashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::error;

use crate::dedlog::sanitizer::{Sanitizer, WithCollapseSpaces};
use crate::dedlog::consts;


/// Log entry for deduplication
pub struct LogEntry {
    err: Option<String>,
    reason: String,
    extra: Option<String>,
    count: usize,
}

impl LogEntry {
    #[allow(dead_code)] // Used internally in err() function
    fn new(err: Option<String>, extra: Option<String>, reason: String) -> Self {
        Self {
            err,
            reason,
            extra,
            count: 1,
        }
    }
}

// Global channel for sending log entries
// We'll initialize this when start_dedup_logger is called
// Use std::sync::Mutex for synchronous access on hotpath
static ERR_CH: once_cell::sync::Lazy<Arc<Mutex<Option<mpsc::Sender<LogEntry>>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

fn get_err_ch() -> Option<mpsc::Sender<LogEntry>> {
    if let Ok(guard) = ERR_CH.try_lock() {
        guard.clone()
    } else {
        None
    }
}

fn set_err_ch(tx: mpsc::Sender<LogEntry>) {
    if let Ok(mut guard) = ERR_CH.lock() {
        *guard = Some(tx);
    }
}

/// Hot path method for logging errors without affecting performance.
/// This is a synchronous function that uses non-blocking try_lock and try_send
/// to avoid blocking the hotpath.
pub fn err(err: Option<&dyn std::error::Error>, extra: Option<&str>, msg: &str) {
    if let Some(tx) = get_err_ch() {
        let entry = LogEntry::new(
            err.map(|e| e.to_string()),
            extra.map(|s| s.to_string()),
            msg.to_string(),
        );
        // Non-blocking send - try_send on tokio::sync::mpsc::Sender is synchronous
        let _ = tx.try_send(entry);
    }
}

/// Starts the deduplicated logger in a background task.
pub async fn start_dedup_logger(ctx: CancellationToken) {
    let (tx, mut rx) = mpsc::channel(1024);
    set_err_ch(tx);
    let mut prev_map: Arc<DashMap<String, LogEntry>> = Arc::new(DashMap::new());
    let mut cur_map: Arc<DashMap<String, LogEntry>> = Arc::new(DashMap::new());

    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let sanitizer = Sanitizer::new(WithCollapseSpaces(true));

    loop {
        tokio::select! {
            _ = ctx.cancelled() => {
                break;
            }
            entry = rx.recv() => {
                if let Some(entry) = entry {
                    if let Some(mut existing) = cur_map.get_mut(&entry.reason) {
                        existing.count += 1;
                    } else {
                        cur_map.insert(entry.reason.clone(), entry);
                    }
                }
            }
            _ = interval.tick() => {
                // Swap maps and log previous entries
                // Clone Arc for previous map before swapping
                let prev = Arc::clone(&prev_map);
                prev_map = Arc::clone(&cur_map);
                cur_map = Arc::new(DashMap::new());

                // Log all entries from previous period
                for entry in prev.iter() {
                    if let Some(err) = &entry.err {
                        let sanitized_err = sanitizer.sanitize(err);
                        if let Some(extra) = &entry.extra {
                            error!(
                                component = consts::COMPONENT,
                                count = entry.count,
                                err = %sanitized_err,
                                extra = %extra,
                                "{}", entry.reason
                            );
                        } else {
                            error!(
                                component = consts::COMPONENT,
                                count = entry.count,
                                err = %sanitized_err,
                                "{}", entry.reason
                            );
                        }
                    } else if let Some(extra) = &entry.extra {
                        error!(
                            component = consts::COMPONENT,
                            count = entry.count,
                            extra = %extra,
                            "{}", entry.reason
                        );
                    } else {
                        error!(
                            component = consts::COMPONENT,
                            count = entry.count,
                            "{}", entry.reason
                        );
                    }
                }
                // Explicitly drop prev to ensure DashMap is freed immediately after logging
                // This prevents holding onto the previous period's data longer than necessary
                drop(prev);
            }
        }
    }
}
