//! HTTP header filtering.

use crate::config::Rule;
use crate::sort::key_value::kv_slice;

/// Filters and sorts request headers based on rule configuration.
pub fn filter_and_sort_request(
    rule: Option<&Rule>,
    headers: &[(String, String)],
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
        let k_lower = k.to_lowercase();
        if allowed_map.contains_key(&k_lower) {
            out.push((k.as_bytes().to_vec(), v.as_bytes().to_vec()));
        }
    }

    // Sort if more than one entry using insertion sort
    if out.len() > 1 {
        kv_slice(&mut out);
    }

    out
}
