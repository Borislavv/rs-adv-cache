//! Lifetime manager controller.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::config::{Config, ConfigTrait};
use crate::governor::Governor;
use crate::http::Controller;
use crate::db::SVC_LIFETIME_MANAGER;

/// Query parameters for scale endpoint.
#[derive(Deserialize)]
struct ScaleQuery {
    to: Option<String>,
}

/// Query parameters for rate endpoint.
#[derive(Deserialize)]
struct RateQuery {
    to: Option<String>,
}

/// LifetimeManagerController handles lifetime manager worker control.
pub struct LifetimeManagerController {
    cfg: Arc<Config>,
    orchestrator: Arc<dyn Governor>,
}

impl LifetimeManagerController {
    /// Creates a new lifetime manager controller.
    pub fn new(cfg: Config, orchestrator: Arc<dyn Governor>) -> Self {
        Self {
            cfg: Arc::new(cfg),
            orchestrator,
        }
    }

    /// Gets the current lifetime manager configuration.
    async fn get(State(controller): State<Arc<Self>>) -> impl IntoResponse {
        match controller.orchestrator.cfg(SVC_LIFETIME_MANAGER) {
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

    /// Enables lifetime manager.
    async fn on(State(controller): State<Arc<Self>>) -> Response {
        match controller.orchestrator.on(SVC_LIFETIME_MANAGER) {
            Ok(_) => Self::get(State(controller)).await.into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain")],
                e.to_string(),
            )
                .into_response(),
        }
    }

    /// Disables lifetime manager.
    async fn off(State(controller): State<Arc<Self>>) -> Response {
        match controller.orchestrator.off(SVC_LIFETIME_MANAGER) {
            Ok(_) => Self::get(State(controller)).await.into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain")],
                e.to_string(),
            )
                .into_response(),
        }
    }

    /// Scales lifetime manager workers.
    async fn scale(
        Query(params): Query<ScaleQuery>,
        State(controller): State<Arc<Self>>,
    ) -> Response {
        let to = match params.to {
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

        match controller.orchestrator.scale_to(SVC_LIFETIME_MANAGER, to) {
            Ok(_) => Self::get(State(controller)).await.into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain")],
                e.to_string(),
            )
                .into_response(),
        }
    }

    /// Sets the rate for lifetime manager.
    async fn rate(
        Query(params): Query<RateQuery>,
        State(controller): State<Arc<Self>>,
    ) -> impl IntoResponse {
        let new_rate = match params.to {
            Some(r) => match r.parse::<usize>() {
                Ok(n) => n,
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        [("content-type", "text/plain")],
                        "invalid 'rate' parameter".to_string(),
                    );
                }
            },
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    [("content-type", "text/plain")],
                    "missing 'rate' parameter".to_string(),
                );
            }
        };

        // Get current config and update rate
        match controller.orchestrator.cfg(SVC_LIFETIME_MANAGER) {
            Ok(current_cfg) => {
                // Get current frequency config
                let current_freq = current_cfg.get_freq();

                // Create new frequency with updated rate limit
                let new_freq = current_freq.set_rate_limit(new_rate);

                // Create new config with updated frequency
                let new_cfg = current_cfg.set_freq(new_freq);

                // Reload service with new config
                match controller
                    .orchestrator
                    .reload(SVC_LIFETIME_MANAGER, new_cfg)
                {
                    Ok(_) => (
                        StatusCode::OK,
                        [("content-type", "text/plain")],
                        format!("rate updated to {}", new_rate),
                    ),
                    Err(e) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        [("content-type", "text/plain")],
                        format!("failed to reload service: {}", e),
                    ),
                }
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("content-type", "text/plain")],
                e.to_string(),
            ),
        }
    }

    /// Gets the policy for lifetime manager.
    async fn policy(State(controller): State<Arc<Self>>) -> impl IntoResponse {
        use serde::Serialize;

        #[derive(Serialize)]
        struct LifetimePolicyResponse {
            #[serde(rename = "on_ttl")]
            on_ttl: String,
        }

        let on_ttl = controller
            .cfg
            .as_ref()
            .lifetime()
            .map(|l| {
                if l.is_remove_on_ttl
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    "remove"
                } else {
                    "refresh"
                }
            })
            .unwrap_or("refresh");

        let resp = LifetimePolicyResponse {
            on_ttl: on_ttl.to_string(),
        };

        (
            StatusCode::OK,
            [("content-type", "application/json")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }

    /// Sets policy to remove on TTL.
    async fn to_remove_policy(State(controller): State<Arc<Self>>) -> impl IntoResponse {
        if let Some(lifetime) = controller.cfg.lifetime() {
            lifetime
                .is_remove_on_ttl
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Self::policy(State(controller)).await
    }

    /// Sets policy to refresh on TTL.
    async fn to_refresh_policy(State(controller): State<Arc<Self>>) -> impl IntoResponse {
        if let Some(lifetime) = controller.cfg.lifetime() {
            lifetime
                .is_remove_on_ttl
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
        Self::policy(State(controller)).await
    }
}

impl Controller for LifetimeManagerController {
    fn add_route(&self, router: Router) -> Router {
        let controller1 = Arc::new(self.clone());
        let controller2 = Arc::new(self.clone());
        let controller3 = Arc::new(self.clone());
        let controller4 = Arc::new(self.clone());
        let controller5 = Arc::new(self.clone());
        let controller6 = Arc::new(self.clone());
        let controller7 = Arc::new(self.clone());
        let controller8 = Arc::new(self.clone());
        router
            .route(
                "/advcache/lifetime-manager",
                get(move || {
                    let controller = controller1.clone();
                    async move { Self::get(State(controller)).await }
                }),
            )
            .route(
                "/advcache/lifetime-manager/on",
                get(move || {
                    let controller = controller2.clone();
                    async move { Self::on(State(controller)).await }
                }),
            )
            .route(
                "/advcache/lifetime-manager/off",
                get(move || {
                    let controller = controller3.clone();
                    async move { Self::off(State(controller)).await }
                }),
            )
            .route(
                "/advcache/lifetime-manager/scale",
                get(move |query: Query<ScaleQuery>| {
                    let controller = controller4.clone();
                    async move { Self::scale(query, State(controller)).await }
                }),
            )
            .route(
                "/advcache/lifetime-manager/rate",
                get(move |query: Query<RateQuery>| {
                    let controller = controller5.clone();
                    async move { Self::rate(query, State(controller)).await }
                }),
            )
            .route(
                "/advcache/lifetime-manager/policy",
                get(move || {
                    let controller = controller6.clone();
                    async move { Self::policy(State(controller)).await }
                }),
            )
            .route(
                "/advcache/lifetime-manager/policy/remove",
                get(move || {
                    let controller = controller7.clone();
                    async move { Self::to_remove_policy(State(controller)).await }
                }),
            )
            .route(
                "/advcache/lifetime-manager/policy/refresh",
                get(move || {
                    let controller = controller8.clone();
                    async move { Self::to_refresh_policy(State(controller)).await }
                }),
            )
    }
}

impl Clone for LifetimeManagerController {
    fn clone(&self) -> Self {
        Self {
            cfg: self.cfg.clone(),
            orchestrator: self.orchestrator.clone(),
        }
    }
}
