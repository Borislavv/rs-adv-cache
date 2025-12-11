// Package api provides config display controller.

use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use std::sync::Arc;

use crate::config::Config;
use crate::http::Controller;

/// ShowConfigController displays the current configuration.
pub struct ShowConfigController {
    cfg: Arc<Config>,
}

impl ShowConfigController {
    /// Creates a new show config controller.
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg: Arc::new(cfg),
        }
    }

    /// Handles the show config request.
    async fn show_config(cfg: Arc<Config>) -> impl IntoResponse {
        let json = serde_json::to_string(&*cfg)
            .unwrap_or_else(|_| r#"{"error": "failed to serialize config"}"#.to_string());
        
        (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            json,
        )
    }
}

impl Controller for ShowConfigController {
    fn add_route(&self, router: Router) -> Router {
        let cfg = self.cfg.clone();
        router
            .route("/advcache/config", get(move || {
                let cfg = cfg.clone();
                async move { Self::show_config(cfg).await }
            }))
    }
}

