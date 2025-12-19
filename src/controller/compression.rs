//! HTTP compression controller.

use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use serde::Serialize;

use crate::http::Controller;
use crate::http::{disable_compression, enable_compression, is_compression_enabled};

/// Status response structure.
#[derive(Debug, Serialize)]
struct StatusResponse {
    enabled: bool,
    message: String,
}

/// HttpCompressionController handles HTTP compression toggling.
pub struct HttpCompressionController;

impl HttpCompressionController {
    /// Creates a new HTTP compression controller.
    pub fn new() -> Self {
        Self
    }

    /// Gets the current compression status.
    async fn get() -> impl IntoResponse {
        let resp = StatusResponse {
            enabled: is_compression_enabled(),
            message: "compression".to_string(),
        };
        (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }

    /// Enables compression.
    async fn on() -> impl IntoResponse {
        enable_compression();
        let resp = StatusResponse {
            enabled: is_compression_enabled(),
            message: "compression".to_string(),
        };
        (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }

    /// Disables compression.
    async fn off() -> impl IntoResponse {
        disable_compression();
        let resp = StatusResponse {
            enabled: is_compression_enabled(),
            message: "compression".to_string(),
        };
        (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }
}

impl Default for HttpCompressionController {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller for HttpCompressionController {
    fn add_route(&self, router: Router) -> Router {
        router
            .route("/advcache/http/compression", get(Self::get))
            .route("/advcache/http/compression/on", get(Self::on))
            .route("/advcache/http/compression/off", get(Self::off))
    }
}
