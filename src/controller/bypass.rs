//! Cache bypass (on/off) controller.

use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use serde::Serialize;
use std::sync::Arc;

use crate::config::{Config, ConfigTrait};
use crate::http::Controller;

/// Status response for bypass operations.
#[derive(Debug, Serialize)]
struct StatusResponse {
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// OnOffController provides endpoints to switch the advanced cache on and off.
pub struct BypassOnOffController {
    cfg: Arc<Config>,
}

impl BypassOnOffController {
    /// Creates a new OnOffController instance.
    pub fn new(cfg: Config) -> Self {
        Self { cfg: Arc::new(cfg) }
    }

    /// Handles POST /adv-cache/on and enables the advanced cache, returning JSON.
    async fn on(cfg: Arc<Config>) -> impl IntoResponse {
        cfg.set_enabled(true);
        let resp = StatusResponse {
            enabled: !cfg.is_enabled(),
            message: Some("bypass".to_string()),
        };
        (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }

    /// Handles POST /adv-cache/off and disables the advanced cache, returning JSON.
    async fn off(cfg: Arc<Config>) -> impl IntoResponse {
        cfg.set_enabled(false);
        let resp = StatusResponse {
            enabled: !cfg.is_enabled(),
            message: Some("bypass".to_string()),
        };
        (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }

    /// Handles GET /cache/bypass and returns current bypass status.
    async fn bypass_is(cfg: Arc<Config>) -> impl IntoResponse {
        let resp = StatusResponse {
            enabled: !cfg.is_enabled(),
            message: Some("bypass".to_string()),
        };
        (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }
}

impl Controller for BypassOnOffController {
    fn add_route(&self, router: Router) -> Router {
        let cfg1 = self.cfg.clone();
        let cfg2 = self.cfg.clone();
        let cfg3 = self.cfg.clone();
        let router = router
            .route(
                "/advcache/bypass/on",
                get(move || {
                    let cfg = cfg1.clone();
                    async move { Self::off(cfg).await }
                }),
            )
            .route(
                "/advcache/bypass/off",
                get(move || {
                    let cfg = cfg2.clone();
                    async move { Self::on(cfg).await }
                }),
            )
            .route(
                "/advcache/bypass",
                get(move || {
                    let cfg = cfg3.clone();
                    async move { Self::bypass_is(cfg).await }
                }),
            )
            .route(
                "/cache/bypass/on",
                get({
                    let cfg = self.cfg.clone();
                    move || {
                        let cfg = cfg.clone();
                        async move { Self::off(cfg).await }
                    }
                }),
            )
            .route(
                "/cache/bypass/off",
                get({
                    let cfg = self.cfg.clone();
                    move || {
                        let cfg = cfg.clone();
                        async move { Self::on(cfg).await }
                    }
                }),
            )
            .route(
                "/cache/bypass",
                get({
                    let cfg = self.cfg.clone();
                    move || {
                        let cfg = cfg.clone();
                        async move { Self::bypass_is(cfg).await }
                    }
                }),
            );

        router
    }
}
