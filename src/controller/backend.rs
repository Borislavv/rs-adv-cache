//! Backend policy controller.

use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use serde::Serialize;

use crate::http::Controller;
use crate::upstream::{actual_policy, change_policy, Policy};

/// Backend policy response structure.
#[derive(Debug, Serialize)]
struct ShowBackendPolicy {
    current: String,
}

impl ShowBackendPolicy {
    fn new(policy: Policy) -> Self {
        Self {
            current: match policy {
                Policy::Await => "await".to_string(),
                Policy::Deny => "deny".to_string(),
            },
        }
    }
}

/// ChangeBackendPolicyController handles upstream policy changes.
pub struct ChangeBackendPolicyController;

impl ChangeBackendPolicyController {
    /// Creates a new change backend policy controller.
    pub fn new() -> Self {
        Self
    }

    /// Turns on await policy.
    async fn turn_on_await_policy() -> impl IntoResponse {
        let _ = change_policy(Policy::Await);
        let resp = ShowBackendPolicy::new(actual_policy());
        (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }

    /// Turns on deny policy.
    async fn turn_on_deny_policy() -> impl IntoResponse {
        let _ = change_policy(Policy::Deny);
        let resp = ShowBackendPolicy::new(actual_policy());
        (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }

    /// Shows the current policy.
    async fn show_policy() -> impl IntoResponse {
        let resp = ShowBackendPolicy::new(actual_policy());
        (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }
}

impl Default for ChangeBackendPolicyController {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller for ChangeBackendPolicyController {
    fn add_route(&self, router: Router) -> Router {
        router
            .route(
                "/advcache/upstream/policy/await",
                get(Self::turn_on_await_policy),
            )
            .route(
                "/advcache/upstream/policy/deny",
                get(Self::turn_on_deny_policy),
            )
            .route("/advcache/upstream/policy", get(Self::show_policy))
    }
}
