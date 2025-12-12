use axum::http::{HeaderMap, HeaderName};

use crate::config::Rule;

/// Hop-by-hop headers that must not be forwarded by proxies (RFC 7230, section 6.1).
#[allow(dead_code)]
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

/// Sanitizes hop-by-hop headers from request.
#[allow(dead_code)]
pub fn sanitize_hop_by_hop_request_headers(headers: &mut HeaderMap) {
    for &header_name in HOP_BY_HOP {
        if let Ok(name) = HeaderName::try_from(header_name.as_bytes()) {
            headers.remove(name);
        }
    }
}

/// Sanitizes hop-by-hop headers from response.
#[allow(dead_code)]
pub fn sanitize_hop_by_hop_response_headers(headers: &mut HeaderMap) {
    sanitize_hop_by_hop_request_headers(headers);
}

/// Sanitizes response headers based on rule configuration.
#[allow(dead_code)]
pub fn sanitize_response_headers_by_rule(rule: Option<&Rule>, headers: &mut HeaderMap) {
    let rule = match rule {
        Some(r) => r,
        None => return,
    };

    let allowed_map = match &rule.cache_value.headers_map {
        Some(map) => map,
        None => return,
    };

    if allowed_map.is_empty() {
        return;
    }

    // Collect headers to remove
    let mut to_remove = Vec::new();
    for (name, _value) in headers.iter() {
        let name_str = name.as_str().to_lowercase();
        if !allowed_map.contains(&name_str) && !allowed_map.contains(name.as_str()) {
            to_remove.push(name.clone());
        }
    }

    // Remove disallowed headers
    for name in to_remove {
        headers.remove(name);
    }
}

