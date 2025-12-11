// Status codes are tracked via labels, not separate metric names.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

/// Static metric name for response status codes.
const RESP_STATUS_TOTAL: &str = "resp_status_total";

/// Status code counters for metrics.
/// Use Lazy to initialize Vec of AtomicU64
static STATUS_CODE_COUNTERS: Lazy<Vec<AtomicU64>> = Lazy::new(|| {
    (0..599).map(|_| AtomicU64::new(0)).collect()
});

/// Increments status code counter.
pub fn inc_status_code(code: u16) {
    if code < 599 {
        if let Some(counter) = STATUS_CODE_COUNTERS.get(code as usize) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Flushes status code counters to metrics.
pub fn flush_status_code_counters() {
    use metrics::{Key, KeyName, Label};
    
    // Register the counter with label support (lazy initialization)
    static COUNTER_REGISTERED: std::sync::Once = std::sync::Once::new();
    COUNTER_REGISTERED.call_once(|| {
        metrics::describe_counter!(
            RESP_STATUS_TOTAL,
            metrics::Unit::Count,
            "Total number of HTTP responses by status code"
        );
    });
    
    for (code, counter) in STATUS_CODE_COUNTERS.iter().enumerate() {
        let count = counter.swap(0, Ordering::Relaxed);
        if count > 0 {
            // Format code as string for label value.
            let code_str = code.to_string();
            
            // Create Key with static name and label.
            let label = Label::new("code", code_str);
            let key = Key::from_name(KeyName::from(RESP_STATUS_TOTAL))
                .with_extra_labels(vec![label]);
            
            // Use the recorder API to increment the counter with labels
            if let Some(recorder) = metrics::try_recorder() {
                let counter_handle = recorder.register_counter(&key);
                counter_handle.increment(count);
            } else {
                metrics::counter!(RESP_STATUS_TOTAL, count);
            }
        }
    }
}

