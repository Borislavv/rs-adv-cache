// Package header provides HTTP header filtering.

use crate::config::Rule;

/// Filters and sorts request headers based on rule configuration.
pub fn filter_and_sort_request(
    rule: Option<&Rule>,
    headers: &[(String, String)],
) -> Vec<(Vec<u8>, Vec<u8>)> {
    if rule.is_none() {
        return Vec::new()
    }

    let rule = rule.unwrap();
    let allowed_map = match &rule.cache_key.headers_map {
        Some(map) => map,
        None => return Vec::new(),
    };

    if allowed_map.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();

    for (k, v) in headers {
        let k_lower = k.to_lowercase();
        if allowed_map.contains_key(&k_lower) || allowed_map.contains_key(&k.to_lowercase()) {
            out.push((k.as_bytes().to_vec(), v.as_bytes().to_vec()));
        }
    }

    // Sort if more than one entry
    if out.len() > 1 {
        out.sort_by(|a, b| a.0.cmp(&b.0));
    }

    out
}

