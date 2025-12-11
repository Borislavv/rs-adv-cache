// Package api provides cache entry retrieval controller.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::http::Controller;
use crate::storage::Storage;

/// Query parameters for get endpoint.
#[derive(Deserialize)]
struct GetQuery {
    key: Option<String>,
}

/// GetController handles cache entry retrieval by key.
pub struct GetController {
    db: Arc<dyn Storage>,
}

impl GetController {
    /// Creates a new get controller.
    pub fn new(db: Arc<dyn Storage>) -> Self {
        Self { db }
    }

    /// Handles the get request.
    async fn get(
        Query(params): Query<GetQuery>,
        State(controller): State<Arc<Self>>,
    ) -> impl IntoResponse {
        let key_str = match params.key {
            Some(k) => k,
            None => {
                return (StatusCode::NOT_FOUND, "").into_response();
            }
        };

        let key = match key_str.parse::<u64>() {
            Ok(k) => k,
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "").into_response();
            }
        };

        let (entry, hit) = controller.db.get_by_key(key);
        
        if !hit {
            return (StatusCode::NOT_FOUND, "").into_response();
        }

        if let Some(_entry) = entry {
            let json = serde_json::json!({
                "key": key,
                "found": true,
            });
            
            (StatusCode::OK, Json(json)).into_response()
        } else {
            (StatusCode::NOT_FOUND, "").into_response()
        }
    }
}

impl Controller for GetController {
    fn add_route(&self, router: Router) -> Router {
        let controller = Arc::new(self.clone());
        router
            .route("/advcache/entry", get(move |query: Query<GetQuery>| {
                let controller = controller.clone();
                async move {
                    Self::get(query, State(controller)).await
                }
            }))
    }
}

impl Clone for GetController {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
        }
    }
}

