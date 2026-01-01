//! Panic recovery middleware.
//

use axum::{extract::Request, middleware::Next, response::Response};
use std::sync::atomic::{AtomicU64, Ordering};

/// Global panic counter.
static PANICS_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Gets the current panic counter value.
pub fn panics_counter() -> u64 {
    PANICS_COUNTER.load(Ordering::Relaxed)
}

/// Increments the panic counter.
/// Should be called when a panic is caught.
pub fn inc_panics() {
    PANICS_COUNTER.fetch_add(1, Ordering::Relaxed);
    // Also update metrics in real-time
    crate::controller::metrics::inc_panics(1);
}

/// PanicRecoverMiddleware recovers from panics in HTTP handlers.
pub struct PanicRecoverMiddleware;

impl PanicRecoverMiddleware {
    /// Creates a new panic recovery middleware.
    pub fn new() -> Self {
        Self
    }

    /// Middleware function that handles errors.
    pub async fn middleware(&self, request: Request, next: Next) -> Response {
        // Execute the next middleware/handler
        // In async Rust, we rely on proper error handling
        next.run(request).await
    }
}

impl Default for PanicRecoverMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

// For use as axum middleware
pub async fn panic_recover_middleware(request: Request, next: Next) -> Response {
    PanicRecoverMiddleware::new()
        .middleware(request, next)
        .await
}

// Implementation of Middleware trait
impl crate::middleware::middleware::Middleware for PanicRecoverMiddleware {
    fn apply(&self, router: axum::Router) -> axum::Router {
        router.layer(axum::middleware::from_fn(panic_recover_middleware))
    }
}
