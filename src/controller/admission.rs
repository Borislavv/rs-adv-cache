// Package api provides admission control controller.

use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Serialize;
use std::sync::Arc;
use crate::config::{Config, ConfigTrait};
use crate::http::Controller;

/// Admission response structure.
#[derive(Debug, Serialize)]
struct AdmissionResponse {
    #[serde(rename = "is_active")]
    is_active: bool,
}

/// AdmissionController handles admission control toggling.
pub struct AdmissionController {
    cfg: Arc<Config>,
}

impl AdmissionController {
    /// Creates a new admission controller.
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg: Arc::new(cfg),
        }
    }

    /// Gets the current admission status.
    async fn get(cfg: Arc<Config>) -> impl IntoResponse {
        let is_active = cfg.admission()
            .map(|a| a.is_enabled.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false);

        let resp = AdmissionResponse { is_active };
        (
            StatusCode::OK,
            [("content-type", "application/json")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }

    /// Enables admission control.
    async fn on(cfg: Arc<Config>) -> impl IntoResponse {
        if let Some(admission) = cfg.admission() {
            admission.is_enabled.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Self::get(cfg).await
    }

    /// Disables admission control.
    async fn off(cfg: Arc<Config>) -> impl IntoResponse {
        if let Some(admission) = cfg.admission() {
            admission.is_enabled.store(false, std::sync::atomic::Ordering::Relaxed);
        }
        Self::get(cfg).await
    }
}

impl Controller for AdmissionController {
    fn add_route(&self, router: Router) -> Router {
        let cfg1 = self.cfg.clone();
        let cfg2 = self.cfg.clone();
        let cfg3 = self.cfg.clone();
        router
            .route("/advcache/admission", get(move || {
                async move { Self::get(cfg1).await }
            }))
            .route("/advcache/admission/on", get(move || {
                async move { Self::on(cfg2).await }
            }))
            .route("/advcache/admission/off", get(move || {
                async move { Self::off(cfg3).await }
            }))
    }
}

