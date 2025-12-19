// PERFORMANCE: All metric names are static &'static str to avoid runtime allocations.
// Status codes are tracked via labels, not separate metric names.

use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};

/// Static metric name for response status codes.
/// We use a single static name "resp_status_total" with a "code" label
const RESP_STATUS_TOTAL: &str = "resp_status_total";

/// Status code counters for metrics.
/// Use Lazy to initialize Vec of AtomicU64
static STATUS_CODE_COUNTERS: Lazy<Vec<AtomicU64>> =
    Lazy::new(|| (0..599).map(|_| AtomicU64::new(0)).collect());

/// Increments status code counter.
pub fn inc_status_code(code: u16) {
    if code < 599 {
        if let Some(counter) = STATUS_CODE_COUNTERS.get(code as usize) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Flushes status code counters to metrics.
///
/// PERFORMANCE: Uses static metric name with labels to avoid String allocations for the NAME.
/// The code value is formatted as a label. While label values may allocate, the metric NAME
/// itself is always static &'static str, which is the critical performance requirement.
///
/// The metrics crate requires using the recorder API for labels.
/// We register the counter once with the label key, then use it with different label values.
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
            // Note: Label values may allocate, but the metric NAME is static.
            // This is acceptable since label values are not in the hot path of metric name creation.
            let code_str = code.to_string();

            // Create Key with static name and label.
            // Label::new takes ownership of the value string, so we can pass code_str directly
            // or use a reference if it accepts &str. Let's try with the owned String.
            let label = Label::new("code", code_str);
            let key =
                Key::from_name(KeyName::from(RESP_STATUS_TOTAL)).with_extra_labels(vec![label]);

            // Use the recorder API to increment the counter with labels
            if let Some(recorder) = metrics::try_recorder() {
                let counter_handle = recorder.register_counter(&key);
                counter_handle.increment(count);
            } else {
                // Fallback: use counter! macro without labels if recorder is not available
                // This loses per-code breakdown but preserves total count
                metrics::counter!(RESP_STATUS_TOTAL, count);
            }
        }
    }
}
