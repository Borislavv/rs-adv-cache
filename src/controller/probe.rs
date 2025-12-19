//! Liveness probe controller.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::sync::Arc;

use crate::http::Controller;
use crate::liveness;

const SUCCESS_RESPONSE: &str = r#"{
  "status": 200,
  "message": "I'm fine :D"
}"#;

const FAILED_RESPONSE: &str = r#"{
  "status": 503,
  "message": "I'm tired :("
}"#;

/// LivenessProbeController handles Kubernetes liveness probes.
pub struct LivenessProbeController {
    probe: Arc<dyn liveness::Prober>,
}

impl LivenessProbeController {
    /// Creates a new liveness probe controller.
    pub fn new(probe: Arc<dyn liveness::Prober>) -> Self {
        Self { probe }
    }

    /// Handles the probe request.
    async fn probe(&self) -> Response {
        if self.probe.is_alive() {
            (StatusCode::OK, SUCCESS_RESPONSE).into_response()
        } else {
            (StatusCode::SERVICE_UNAVAILABLE, FAILED_RESPONSE).into_response()
        }
    }
}

impl Controller for LivenessProbeController {
    fn add_route(&self, router: Router) -> Router {
        let probe_controller = self.clone();
        router
            .route(
                "/k8s/probe",
                get({
                    let controller = probe_controller.clone();
                    move || {
                        let controller = controller.clone();
                        async move { controller.probe().await }
                    }
                }),
            )
            .route(
                "/healthz",
                get({
                    let controller = probe_controller.clone();
                    move || {
                        let controller = controller.clone();
                        async move { controller.probe().await }
                    }
                }),
            )
    }
}

impl Clone for LivenessProbeController {
    fn clone(&self) -> Self {
        Self {
            probe: self.probe.clone(),
        }
    }
}
