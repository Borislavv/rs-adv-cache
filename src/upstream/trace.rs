// Package upstream provides tracing functionality for upstream requests.

use tracing::{Span, Level};

use crate::config::Rule;
use crate::model::Entry;

/// Starts a proxy request span.
#[allow(dead_code)]
pub fn start_proxy_request_span(
    path: &str,
    request_str: &str,
) -> Span {
    tracing::span!(
        Level::INFO,
        "upstream",
        "http.path" = path,
        "http.request" = request_str,
    )
}

/// Starts a request span.
#[allow(dead_code)]
pub fn start_request_span(
    rule: &Rule,
    request_str: &str,
) -> Span {
    tracing::span!(
        Level::INFO,
        "upstream",
        "http.path" = rule.path.as_deref().unwrap_or(""),
        "http.request" = request_str,
    )
}

/// Starts a refresh span context.
pub fn start_refresh_span_context(entry: &Entry) -> Span {
    tracing::span!(Level::INFO, "refresh", "cache.key" = entry.key())
}

/// Records response information in span.
#[allow(dead_code)]
pub fn record_response_in_span(span: &Span, status_code: u16, response_size: usize) {
    span.record("http.status_code", status_code);
    span.record("http.response_size", response_size);
}

/// Records error in span.
pub fn record_error_in_span(span: &Span, err: &dyn std::error::Error) {
    tracing::error!(parent: span, error = %err, "upstream error");
}

