// Package api provides traces controller.

use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Serialize;

use crate::http::Controller;
use crate::traces;

/// Traces response structure.
#[derive(Debug, Serialize)]
struct TracesResponse {
    #[serde(rename = "is_active")]
    is_active: bool,
}

/// TracesController handles tracing toggling.
pub struct TracesController;

impl TracesController {
    /// Creates a new traces controller.
    pub fn new() -> Self {
        Self
    }

    /// Gets the current traces status.
    async fn get() -> impl IntoResponse {
        let is_active = traces::is_active_tracing();
        let resp = TracesResponse { is_active };
        (
            StatusCode::OK,
            [("content-type", "application/json")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }

    /// Enables tracing.
    async fn on() -> impl IntoResponse {
        traces::enable_tracing();
        Self::get().await
    }

    /// Disables tracing.
    async fn off() -> impl IntoResponse {
        traces::disable_tracing();
        Self::get().await
    }
}

impl Default for TracesController {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller for TracesController {
    fn add_route(&self, router: Router) -> Router {
        router
            .route("/advcache/traces", get(Self::get))
            .route("/advcache/traces/on", get(Self::on))
            .route("/advcache/traces/off", get(Self::off))
    }
}

