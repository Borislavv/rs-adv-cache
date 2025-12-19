//! Eviction controller.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::governor::Governor;
use crate::http::Controller;
use crate::db::SVC_EVICTOR;

/// Query parameters for scale endpoint.
#[derive(Deserialize)]
struct ScaleQuery {
    to: Option<String>,
}

/// EvictionController handles eviction worker control.
pub struct EvictionController {
    orchestrator: Arc<dyn Governor>,
}

impl EvictionController {
    /// Creates a new eviction controller.
    pub fn new(orchestrator: Arc<dyn Governor>) -> Self {
        Self { orchestrator }
    }

    /// Gets the current eviction configuration.
    async fn get(State(controller): State<Arc<Self>>) -> impl IntoResponse {
        match controller.orchestrator.cfg(SVC_EVICTOR) {
            Ok(cfg) => {
                let freq = cfg.get_freq();
                let json = serde_json::json!({
                    "enabled": cfg.is_enabled(),
                    "replicas": cfg.get_replicas(),
                    "frequency": {
                        "limit_per_sec": freq.get_rate_limit(),
                        "interval_ns": freq.get_tick_freq().as_nanos(),
                    }
                });
                (
                    StatusCode::OK,
                    [("content-type", "application/json")],
                    serde_json::to_string(&json).unwrap_or_default(),
                )
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain")],
                e.to_string(),
            ),
        }
    }

    /// Enables eviction.
    async fn on(State(controller): State<Arc<Self>>) -> Response {
        match controller.orchestrator.on(SVC_EVICTOR) {
            Ok(_) => Self::get(State(controller)).await.into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain")],
                e.to_string(),
            )
                .into_response(),
        }
    }

    /// Disables eviction.
    async fn off(State(controller): State<Arc<Self>>) -> Response {
        match controller.orchestrator.off(SVC_EVICTOR) {
            Ok(_) => Self::get(State(controller)).await.into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain")],
                e.to_string(),
            )
                .into_response(),
        }
    }

    /// Scales eviction workers.
    async fn scale(
        Query(params): Query<ScaleQuery>,
        State(controller): State<Arc<Self>>,
    ) -> Response {
        let to_str = params.to.as_deref();
        let to = match to_str {
            Some(t) => match t.parse::<usize>() {
                Ok(n) => n,
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        [("content-type", "text/plain")],
                        "invalid 'to' parameter".to_string(),
                    )
                        .into_response();
                }
            },
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    [("content-type", "text/plain")],
                    "missing 'to' parameter".to_string(),
                )
                    .into_response();
            }
        };

        if to == 0 && to_str != Some("0") {
            return (
                StatusCode::BAD_REQUEST,
                [("content-type", "text/plain")],
                "'to' value must be positive".to_string(),
            )
                .into_response();
        }

        match controller.orchestrator.scale_to(SVC_EVICTOR, to) {
            Ok(_) => Self::get(State(controller)).await.into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain")],
                e.to_string(),
            )
                .into_response(),
        }
    }
}

impl Controller for EvictionController {
    fn add_route(&self, router: Router) -> Router {
        let controller1 = Arc::new(self.clone());
        let controller2 = Arc::new(self.clone());
        let controller3 = Arc::new(self.clone());
        let controller4 = Arc::new(self.clone());
        router
            .route(
                "/advcache/eviction",
                get(move || {
                    let controller = controller1.clone();
                    async move { Self::get(State(controller)).await }
                }),
            )
            .route(
                "/advcache/eviction/on",
                get(move || {
                    let controller = controller2.clone();
                    async move { Self::on(State(controller)).await }
                }),
            )
            .route(
                "/advcache/eviction/off",
                get(move || {
                    let controller = controller3.clone();
                    async move { Self::off(State(controller)).await }
                }),
            )
            .route(
                "/advcache/eviction/scale",
                get(move |query: Query<ScaleQuery>| {
                    let controller = controller4.clone();
                    async move { Self::scale(query, State(controller)).await }
                }),
            )
    }
}

impl Clone for EvictionController {
    fn clone(&self) -> Self {
        Self {
            orchestrator: self.orchestrator.clone(),
        }
    }
}
