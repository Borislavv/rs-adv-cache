// HTTP controller trait for route registration.

use axum::Router;

/// Trait for adding routes to the HTTP server.
pub trait Controller: Send + Sync {
    /// Adds routes to the router.
    ///
    /// Commonly may be represented as:
    /// ```rust
    /// # use axum::{Router, routing::get};
    /// # async fn handler() -> &'static str { "ok" }
    /// let router: Router<()> = Router::new().route("/path", get(handler));
    /// # let _ = router;
    /// ```
    fn add_route(&self, router: Router) -> Router;
}

