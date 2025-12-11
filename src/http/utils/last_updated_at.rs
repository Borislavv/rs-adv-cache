// Package http provides Last-Updated-At header functionality.

use std::time::{SystemTime, UNIX_EPOCH, Duration};

/// Last-Updated-At header key.
pub const LAST_UPDATED_AT_KEY: &str = "Last-Updated-At";

/// Sets the Last-Updated-At header value (RFC1123) as string.
/// Returns `None` if timestamp is zero or formatting fails.
pub fn set_last_updated_at_value(unix_nano: i64) -> Option<String> {
    if unix_nano == 0 {
        return None;
    }

    // Convert nanoseconds since epoch to SystemTime
    let duration = Duration::from_nanos(unix_nano as u64);
    let time = UNIX_EPOCH + duration;

    // Format as RFC 1123 (HTTP date format)
    // Example: "Mon, 02 Jan 2006 15:04:05 GMT"
    format_rfc1123(time)
}

/// Formats a SystemTime as RFC 1123 date string.
fn format_rfc1123(time: SystemTime) -> Option<String> {
    use chrono::{DateTime, Utc};

    // Convert SystemTime to DateTime<Utc>
    let datetime: DateTime<Utc> = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            let secs = duration.as_secs() as i64;
            let nanos = duration.subsec_nanos();
            DateTime::from_timestamp(secs, nanos)
        }
        Err(_) => return None,
    }?;

    // Format as RFC 1123
    Some(datetime.format("%a, %d %b %Y %H:%M:%S GMT").to_string())
}

