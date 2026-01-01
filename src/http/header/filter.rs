//! HTTP header filtering.

use crate::config::Rule;
use crate::sort::key_value::kv_slice;

/// Filters and sorts request headers based on rule configuration.
/// Accepts headers as byte slices to avoid String allocations.
pub fn filter_and_sort_request(
    rule: Option<&Rule>,
    headers: &[(Vec<u8>, Vec<u8>)],
) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut out = Vec::with_capacity(32);
    
    if rule.is_none() {
        return out;
    }

    let rule = rule.unwrap();
    let allowed_map = match &rule.cache_key.headers_map {
        Some(map) => map,
        None => return out,
    };

    if allowed_map.is_empty() {
        return out;
    }

    for (k, v) in headers {
        // Convert key to lowercase for comparison (headers_map keys are lowercase)
        let k_lower = k.to_ascii_lowercase();
        // Convert to String only for HashMap lookup (headers_map is HashMap<String, Vec<u8>>)
        // This is necessary because headers_map uses String keys, but we avoid String allocation
        // for the header values themselves
        if let Ok(k_lower_str) = String::from_utf8(k_lower) {
            if allowed_map.contains_key(&k_lower_str) {
                // Clone the original key and value (preserving original case for key)
                out.push((k.clone(), v.clone()));
            }
        }
    }

    // Sort if more than one entry using insertion sort
    if out.len() > 1 {
        kv_slice(&mut out);
    }

    out
}
