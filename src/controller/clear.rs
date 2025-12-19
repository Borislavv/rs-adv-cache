//! Cache clear controller.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::config::{Config, ConfigTrait};
use crate::http::Controller;
use crate::db::Storage;
use crate::time;

/// Query parameters for clear endpoint.
#[derive(Deserialize)]
struct ClearQuery {
    token: Option<String>,
}

/// Token response structure.
#[derive(Debug, Serialize)]
struct TokenResponse {
    token: String,
    #[serde(rename = "expiresAt")]
    expires_at: i64,
}

/// Clear status response structure.
#[derive(Debug, Serialize)]
struct ClearStatusResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    cleared: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// ClearController handles cache clearing with token-based security.
pub struct ClearController {
    db: Arc<dyn Storage>,
    cfg: Arc<Config>,
    token_state: Arc<Mutex<TokenState>>,
}

struct TokenState {
    token: Option<String>,
    expires: SystemTime,
}

impl ClearController {
    /// Creates a new clear controller.
    pub fn new(cfg: Config, db: Arc<dyn Storage>) -> Self {
        Self {
            db,
            cfg: Arc::new(cfg),
            token_state: Arc::new(Mutex::new(TokenState {
                token: None,
                expires: SystemTime::UNIX_EPOCH,
            })),
        }
    }

    /// Handles the clear request.
    async fn handle_clear(
        Query(params): Query<ClearQuery>,
        State(controller): State<Arc<Self>>,
    ) -> impl IntoResponse {
        let now = time::now();
        let raw_token = params.token;

        if let Some(token) = raw_token {
            // Validate provided token
            let mut state = controller.token_state.lock().await;

            let valid =
                state.token.as_ref().map(|t| t == &token).unwrap_or(false) && now < state.expires;

            // Clear token after use
            state.token = None;
            state.expires = SystemTime::UNIX_EPOCH;

            if !valid {
                let resp = ClearStatusResponse {
                    cleared: None,
                    error: Some("invalid or expired token".to_string()),
                };

                return (
                    StatusCode::FORBIDDEN,
                    [("content-type", "application/json")],
                    serde_json::to_string(&resp).unwrap_or_default(),
                );
            }

            // Clear storage
            controller.db.clear();

            // Log the clear operation
            if controller.cfg.is_prod() {
                tracing::info!(
                    component = "clear",
                    token = %token,
                    "storage cleared"
                );
            } else {
                tracing::info!(component = "clear", "storage cleared");
            }

            let resp = ClearStatusResponse {
                cleared: Some(true),
                error: None,
            };

            (
                StatusCode::OK,
                [("content-type", "application/json")],
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        } else {
            // Return or reuse token
            let mut state = controller.token_state.lock().await;

            if let Some(ref token) = state.token {
                if now < state.expires {
                    let expires_millis = state
                        .expires
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;

                    let resp = TokenResponse {
                        token: token.clone(),
                        expires_at: expires_millis,
                    };

                    return (
                        StatusCode::OK,
                        [("content-type", "application/json")],
                        serde_json::to_string(&resp).unwrap_or_default(),
                    );
                }
            }

            // Generate new token
            let mut bytes = [0u8; 16];
            rand::thread_rng().fill_bytes(&mut bytes);
            let token = hex::encode(bytes);
            let expires = now + Duration::from_secs(5 * 60);

            state.token = Some(token.clone());
            state.expires = expires;

            let expires_millis = expires.duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;

            let resp = TokenResponse {
                token,
                expires_at: expires_millis,
            };

            (
                StatusCode::OK,
                [("content-type", "application/json")],
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        }
    }
}

impl Controller for ClearController {
    fn add_route(&self, router: Router) -> Router {
        let controller = Arc::new(self.clone());
        router.route(
            "/advcache/clear",
            get(move |query: Query<ClearQuery>| {
                let controller = controller.clone();
                async move { Self::handle_clear(query, State(controller)).await }
            }),
        )
    }
}

impl Clone for ClearController {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            cfg: self.cfg.clone(),
            token_state: self.token_state.clone(),
        }
    }
}
