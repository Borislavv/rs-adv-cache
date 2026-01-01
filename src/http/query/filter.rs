//! HTTP query parameter filtering.

use crate::config::Rule;
use crate::sort::key_value::kv_slice;

/// Normalizes percent encoding hex characters to lowercase (e.g., %2F -> %2f).
/// This ensures case-insensitive percent encoding as per RFC 3986.
/// url::form_urlencoded::parse normalizes to lowercase, so we match that behavior.
/// Accepts bytes and converts to String only when needed for character iteration.
fn normalize_percent_encoding(query_bytes: &[u8]) -> String {
    // Convert to &str for character iteration (necessary for proper UTF-8 handling)
    let query_str = std::str::from_utf8(query_bytes).unwrap_or("");
    // Trim '?' prefix if present
    let query_str = query_str.trim_start_matches('?');
    
    let mut result = String::with_capacity(query_str.len());
    let mut chars = query_str.chars().peekable();
    
    while let Some(ch) = chars.next() {
        if ch == '%' {
            result.push(ch);
            // Read next two characters (hex digits)
            if let Some(d1) = chars.next() {
                if let Some(d2) = chars.next() {
                    // Normalize hex digits to lowercase to match url::form_urlencoded::parse behavior
                    result.push(d1.to_ascii_lowercase());
                    result.push(d2.to_ascii_lowercase());
                    continue;
                }
                result.push(d1);
            }
        } else {
            result.push(ch);
        }
    }
    
    result
}

/// Filters and sorts request query parameters based on rule configuration.
/// Accepts query string as bytes to avoid String allocations at call sites.
pub fn filter_and_sort_request(rule: Option<&Rule>, query_bytes: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut out = Vec::with_capacity(32);
    
    if rule.is_none() {
        return out;
    }

    let rule = rule.unwrap();
    let allowed_keys = match &rule.cache_key.query_bytes {
        Some(keys) => keys,
        None => return out,
    };

    if allowed_keys.is_empty() {
        return out;
    }

    // Normalize percent encoding hex characters to ensure case-insensitive matching
    let normalized_query = normalize_percent_encoding(query_bytes);

    for (key, value) in url::form_urlencoded::parse(normalized_query.as_bytes()) {
        let key_bytes = key.as_bytes();
        if allowed_keys.iter().any(|k| k.as_slice() == key_bytes) {
            out.push((
                key.into_owned().into_bytes(),
                value.into_owned().into_bytes(),
            ));
        }
    }

    // Sort if more than one entry using insertion sort
    if out.len() > 1 {
        kv_slice(&mut out);
    }

    out
}
