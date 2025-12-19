//! Request copying functionality.

use axum::http::{Request, Uri};

/// Copies request from source to destination.
/// Copies request from source to destination.
/// This copies method, URI (path and query), and headers from the source request to the destination.
/// Note: Body copying is handled separately as it requires async operations in axum.
pub fn copy_request_from_source(
    out_req: &mut Request<axum::body::Body>,
    in_req: &Request<axum::body::Body>,
) {
    // Copy method
    *out_req.method_mut() = in_req.method().clone();
    
    // Copy URI (path and query, but not scheme/host which are handled by backend)
    if let Some(path_and_query) = in_req.uri().path_and_query() {
        if let Ok(uri) = Uri::builder()
            .path_and_query(path_and_query.as_str())
            .build() {
            *out_req.uri_mut() = uri;
        }
    }
    
    // Copy headers
    *out_req.headers_mut() = in_req.headers().clone();
}
