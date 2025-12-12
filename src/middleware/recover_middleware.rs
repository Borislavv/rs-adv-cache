// Package http provides panic recovery middleware.

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use futures::FutureExt;
use std::panic;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::error;

const REASON_HEADER_KEY: &str = "X-Error-Reason";
const INTERNAL_SERVER_ERROR_RESPONSE_BODY: &[u8] = b"{\"status\":500,\"error\":\"Internal Server Error\",\"message\":\"Something went wrong. Please contact support immediately.\"}";

/// Global panic counter.
static PANICS_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Gets the current panic counter value.
pub fn panics_counter() -> u64 {
    PANICS_COUNTER.load(Ordering::Relaxed)
}

/// PanicRecoverMiddleware recovers from panics in HTTP handlers.
pub struct PanicRecoverMiddleware;

impl PanicRecoverMiddleware {
    /// Creates a new panic recovery middleware.
    pub fn new() -> Self {
        Self
    }

    /// Middleware function that handles panics.
    pub async fn middleware(&self, request: Request, next: Next) -> Response {
        // Use catch_unwind to catch panics in async context
        let result = panic::AssertUnwindSafe(next.run(request)).catch_unwind().await;

        match result {
            Ok(response) => response,
            Err(panic_info) => {
                PANICS_COUNTER.fetch_add(1, Ordering::Relaxed);

                let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    format!("panic: {}", s)
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    format!("panic: {}", s)
                } else {
                    "panic: unknown".to_string()
                };

                error!(
                    error = %panic_msg,
                    "panic recovered in HTTP handler"
                );

                // Build error response
                let mut headers = HeaderMap::new();
                
                if let (Ok(name), Ok(value)) = (
                    HeaderName::try_from(REASON_HEADER_KEY.as_bytes()),
                    HeaderValue::from_str(&panic_msg),
                ) {
                    headers.insert(name, value);
                }

                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("content-type", "application/json")
                    .header("content-length", INTERNAL_SERVER_ERROR_RESPONSE_BODY.len())
                    .body(INTERNAL_SERVER_ERROR_RESPONSE_BODY.to_vec().into())
                    .map(|mut resp| {
                        *resp.headers_mut() = headers;
                        resp
                    })
                    .unwrap_or_else(|_| {
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Vec::new().into())
                            .unwrap()
                    })
            }
        }
    }
}

impl Default for PanicRecoverMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

// For use as axum middleware
pub async fn panic_recover_middleware(
    request: Request,
    next: Next,
) -> Response {
    PanicRecoverMiddleware::new().middleware(request, next).await
}

// Implementation of Middleware trait
impl crate::middleware::middleware::Middleware for PanicRecoverMiddleware {
    fn apply(&self, router: axum::Router) -> axum::Router {
        router.layer(axum::middleware::from_fn(panic_recover_middleware))
    }
}
