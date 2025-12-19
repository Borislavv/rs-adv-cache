// Proxy forwarding functionality for upstream requests.

use axum::http::HeaderName;

/// X-Forwarded-Host header key (lowercase for HTTP header name validation).
const X_FORWARDED_HOST: &str = "x-forwarded-host";

/// Filters out hop-by-hop headers from headers list (for reqwest RequestBuilder).
/// Returns filtered headers without hop-by-hop headers.
/// Sanitizes hop-by-hop headers from request.
pub fn filter_hop_by_hop_headers(headers: &[(String, String)]) -> Vec<(String, String)> {
    headers
        .iter()
        .filter(|(key, _)| {
            let key_lower = key.to_lowercase();
            !HOP_BY_HOP.contains(&key_lower.as_str())
        })
        .cloned()
        .collect()
}

/// Filters out hop-by-hop headers from headers list (bytes format).
#[allow(dead_code)]
pub fn filter_hop_by_hop_headers_bytes(headers: &[(Vec<u8>, Vec<u8>)]) -> Vec<(Vec<u8>, Vec<u8>)> {
    headers
        .iter()
        .filter(|(key, _)| {
            let key_lower = key.to_ascii_lowercase();
            !HOP_BY_HOP.iter().any(|&h| key_lower == h.as_bytes())
        })
        .cloned()
        .collect()
}

/// Hop-by-hop headers that must not be forwarded by proxies (RFC 7230, section 6.1).
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

/// Extracts forwarded host from source headers, preferring X-Forwarded-Host over Host.
/// Returns the host value to be set as Host header in outgoing request.
pub fn extract_forwarded_host(src_headers: &[(String, String)]) -> Option<String> {
    // Prefer X-Forwarded-Host when present and non-empty.
    for (key, value) in src_headers {
        if key.eq_ignore_ascii_case(X_FORWARDED_HOST) && !value.is_empty() {
            return Some(value.clone());
        }
    }

    // Fall back to Host header.
    for (key, value) in src_headers {
        if key.eq_ignore_ascii_case("host") && !value.is_empty() {
            return Some(value.clone());
        }
    }

    None
}

/// Extracts forwarded host from source headers (bytes format), preferring X-Forwarded-Host over Host.
/// Returns the host value to be set as Host header in outgoing request.
pub fn extract_forwarded_host_bytes(src_headers: &[(Vec<u8>, Vec<u8>)]) -> Option<String> {
    // Prefer X-Forwarded-Host when present and non-empty.
    for (key, value) in src_headers {
        if key.eq_ignore_ascii_case(b"x-forwarded-host") && !value.is_empty() {
            if let Ok(host_str) = String::from_utf8(value.clone()) {
                return Some(host_str);
            }
        }
    }

    // Fall back to Host header.
    for (key, value) in src_headers {
        if key.eq_ignore_ascii_case(b"host") && !value.is_empty() {
            if let Ok(host_str) = String::from_utf8(value.clone()) {
                return Some(host_str);
            }
        }
    }

    None
}

/// Extracts forwarded host value from source headers as bytes (no allocations).
/// Prefers X-Forwarded-Host (case-insensitive) if value is non-empty,
/// otherwise falls back to Host (case-insensitive) if non-empty,
/// otherwise returns None.
pub fn forwarded_host_value_bytes(src_headers: &[(Vec<u8>, Vec<u8>)]) -> Option<&[u8]> {
    // Prefer X-Forwarded-Host when present and non-empty.
    for (key, value) in src_headers {
        if key.eq_ignore_ascii_case(b"x-forwarded-host") && !value.is_empty() {
            return Some(value.as_slice());
        }
    }

    // Fall back to Host header.
    for (key, value) in src_headers {
        if key.eq_ignore_ascii_case(b"host") && !value.is_empty() {
            return Some(value.as_slice());
        }
    }

    None
}

/// Applies forwarded host header to outgoing request, preferring X-Forwarded-Host over Host.
/// Used for HeaderMap-based implementations (tests).
#[allow(dead_code)] // Used in tests
pub fn proxy_forwarded_host(
    dst_headers: &mut axum::http::HeaderMap,
    src_headers: &axum::http::HeaderMap,
) {
    // Prefer X-Forwarded-Host when present and non-empty.
    if let Some(host) = src_headers.get(X_FORWARDED_HOST) {
        if !host.as_bytes().is_empty() {
            if let Ok(host_name) = HeaderName::try_from("host".as_bytes()) {
                dst_headers.insert(host_name, host.clone());
                return;
            }
        }
    }

    // Fall back to Host header.
    if let Some(host) = src_headers.get("host") {
        if let Ok(host_name) = HeaderName::try_from("host".as_bytes()) {
            dst_headers.insert(host_name, host.clone());
        }
    }
}
