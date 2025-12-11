// Package query provides HTTP query parameter filtering.

use crate::config::Rule;

/// Filters and sorts request query parameters based on rule configuration.
pub fn filter_and_sort_request(
    rule: Option<&Rule>,
    query_str: &str,
) -> Vec<(Vec<u8>, Vec<u8>)> {
    if rule.is_none() {
        return Vec::new();
    }

    let rule = rule.unwrap();
    let allowed_keys = match &rule.cache_key.query_bytes {
        Some(keys) => keys,
        None => return Vec::new(),
    };

    if allowed_keys.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();

    // Parse query string
    for pair in query_str.trim_start_matches('?').split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            let key_bytes = key.as_bytes();
            // Check if key matches any allowed key
            for allowed_key in allowed_keys {
                if key_bytes.starts_with(allowed_key) {
                    out.push((key_bytes.to_vec(), value.as_bytes().to_vec()));
                    break;
                }
            }
        }
    }

    // Sort if more than one entry
    if out.len() > 1 {
        out.sort_by(|a, b| a.0.cmp(&b.0));
    }

    out
}

