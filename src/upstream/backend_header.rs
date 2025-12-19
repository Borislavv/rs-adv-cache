//! Optimized header processing for upstream responses.
//! Processes headers directly without intermediate allocations.

use crate::config::Rule;

/// Hop-by-hop header names (lowercase) for fast comparison.
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "proxy-connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// Checks if a header name is hop-by-hop (optimized with case-insensitive comparison).
/// Uses byte-level comparison to avoid allocations.
#[inline]
fn is_hop_by_hop(name: &str) -> bool {
    let name_bytes = name.as_bytes();
    HOP_BY_HOP.iter().any(|&h| {
        let h_bytes = h.as_bytes();
        h_bytes.len() == name_bytes.len() &&
            h_bytes.iter().zip(name_bytes.iter()).all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

/// Processes response headers directly from hyper::Response, filtering hop-by-hop
/// and rule-based headers, returning Vec<(String, String)> efficiently.
pub fn process_response_headers(
    response_headers: &hyper::HeaderMap,
    rule: Option<&Rule>,
) -> Vec<(String, String)> {
    let allowed_map = rule
        .and_then(|r| r.cache_value.headers_map.as_ref())
        .filter(|m| !m.is_empty());

    // Pre-allocate with estimated capacity (most responses have ~10-20 headers)
    let capacity = if let Some(map) = allowed_map {
        map.len().min(32)
    } else {
        response_headers.len().min(32)
    };
    let mut result = Vec::with_capacity(capacity);

    for (name, value) in response_headers.iter() {
        let name_str = name.as_str();
        if is_hop_by_hop(name_str) {
            continue;
        }

        // Filter by rule if present (case-insensitive comparison for HTTP headers)
        if let Some(allowed) = allowed_map {
            if !allowed.iter().any(|h| h.eq_ignore_ascii_case(name_str)) {
                continue;
            }
        }

        // Convert value to string efficiently
        if let Ok(value_str) = value.to_str() {
            result.push((name_str.to_string(), value_str.to_string()));
        }
    }

    result
}
