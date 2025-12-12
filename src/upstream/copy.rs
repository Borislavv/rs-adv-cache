// Package upstream provides request copying functionality.

use axum::http::Request;

/// Copies request from source to destination.
#[allow(dead_code)]
pub fn copy_request_from_source(
    out_req: &mut Request<axum::body::Body>,
    in_req: &Request<axum::body::Body>,
) {
    // This is a helper for copying headers and method
    *out_req.headers_mut() = in_req.headers().clone();
}

