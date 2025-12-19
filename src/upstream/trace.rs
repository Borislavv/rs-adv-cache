//! Tracing functionality for upstream HTTP requests.
//! 
//! Provides span creation and attribute recording for observability of cache misses,
//! proxy requests, and background refresh operations.

use tracing::{Level, Span};

use crate::config::Rule;
use crate::model::Entry;
use crate::traces;

/// Creates a new span for proxy requests (client-side span).
/// 
/// This span represents an outbound HTTP request to the upstream backend.
/// It's a child of the current active span context for request tracing.
pub fn start_proxy_request_span(
    path: &str,
    request_str: &str,
) -> Option<Span> {
    if !traces::is_active_tracing() {
        return None;
    }
    
    Some(tracing::span!(
        Level::INFO,
        "upstream",
        http.path = path,
        http.request = request_str,
    ))
}

/// Creates a new span for cache miss requests (client-side span).
/// 
/// This span represents an outbound HTTP request to the upstream backend
/// triggered by a cache miss. It's a child of the current active span context.
pub fn start_request_span(
    rule: &Rule,
    request_str: &str,
) -> Option<Span> {
    if !traces::is_active_tracing() {
        return None;
    }
    
    let path = rule.path.as_deref().unwrap_or("");
    Some(tracing::span!(
        Level::INFO,
        "upstream",
        http.path = path,
        http.request = request_str,
    ))
}

/// Creates a new span for background refresh operations (internal span).
/// 
/// This span represents a background refresh of an expired cache entry.
/// It's created as a root span for refresh operations that run independently
/// of user requests.
pub fn start_refresh_span_context(entry: &Entry) -> Option<Span> {
    if !traces::is_active_tracing() {
        return None;
    }
    
    Some(tracing::span!(
        Level::INFO,
        "refresh",
        cache.key = entry.key(),
    ))
}

/// Records HTTP response information in the span.
/// 
/// Adds status code and response size as span attributes for observability.
/// Early return if tracing is disabled to avoid unnecessary overhead.
pub fn record_response_in_span(span: &Span, status_code: u16, response_size: usize) {
    // Early return if tracing is disabled
    if !traces::is_active_tracing() {
        return;
    }
    span.record(traces::ATTR_HTTP_STATUS_CODE_KEY, status_code);
    span.record(traces::ATTR_HTTP_RESPONSE_SIZE_KEY, response_size);
}

/// Records an error in the span.
/// 
/// Marks the span with an error attribute and logs the error for observability.
pub fn record_error_in_span(span: &Span, err: &dyn std::error::Error) {
    // Early return if tracing is disabled
    if !traces::is_active_tracing() {
        return;
    }
    span.record("error", true);
    tracing::error!(parent: span, error = %err, "upstream error");
}
