// Package api provides liveness probe controller.

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
    probe: Arc<liveness::Probe>,
}

impl LivenessProbeController {
    /// Creates a new liveness probe controller.
    pub fn new(probe: Arc<liveness::Probe>) -> Self {
        Self { probe }
    }

    /// Handles the probe request.
    async fn probe(&self) -> Response {
        if self.probe.is_alive_async().await {
            (StatusCode::OK, SUCCESS_RESPONSE).into_response()
        } else {
            (StatusCode::SERVICE_UNAVAILABLE, FAILED_RESPONSE).into_response()
        }
    }
}

impl Controller for LivenessProbeController {
    fn add_route(&self, router: Router) -> Router {
        let probe_controller = self.clone();
        router.route("/k8s/probe", get(move || {
            let controller = probe_controller.clone();
            async move {
                controller.probe().await
            }
        }))
    }
}

impl Clone for LivenessProbeController {
    fn clone(&self) -> Self {
        Self {
            probe: self.probe.clone(),
        }
    }
}

