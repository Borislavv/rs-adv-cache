//! Middleware interface.
//

use axum::Router;

/// Middleware trait for HTTP request/response processing.
pub trait Middleware: Send + Sync {
    /// Applies the middleware to the router.
    ///
    /// Applies middleware using tower's Layer pattern.
    fn apply(&self, router: Router) -> Router;
}
