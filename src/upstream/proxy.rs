// Proxy forwarding functionality for upstream requests.

use axum::http::HeaderName;

/// X-Forwarded-Host header key (lowercase for HTTP header name validation).
#[allow(dead_code)] // Used in proxy_forwarded_host function
const X_FORWARDED_HOST: &str = "x-forwarded-host";

/// Applies forwarded host header to outgoing request, preferring X-Forwarded-Host over Host.
#[allow(dead_code)] // Used in tests and may be used in future proxy implementations
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

