// Package api provides cache invalidation controller.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::http::Controller;
use crate::http::query::filter_and_sort_request;
use crate::model::match_cache_rule;
use crate::storage::Storage;

const PATH_SPECIAL: &str = "_path";
const REMOVE_SPECIAL: &str = "_remove";

/// Marked response structure.
#[derive(Debug, Serialize)]
struct MarkedResponse {
    success: bool,
    affected: i64,
}

/// InvalidateController handles cache invalidation and marking.
pub struct InvalidateController {
    db: Arc<dyn Storage>,
    cfg: Arc<Config>,
}

impl InvalidateController {
    /// Creates a new mark outdated controller.
    pub fn new(cfg: Config, db: Arc<dyn Storage>) -> Self {
        Self { db, cfg: Arc::new(cfg) }
    }

    /// Invalidates cache entries based on query parameters and path.
    async fn invalidate(
        Query(params): Query<HashMap<String, String>>,
        State(controller): State<Arc<Self>>,
    ) -> impl IntoResponse {
        // Extract path from parameters
        let path_str = match params.get(PATH_SPECIAL) {
            Some(p) => p.clone(),
            None => {
                let resp = MarkedResponse {
                    success: false,
                    affected: 0,
                };
                return (
                    StatusCode::BAD_REQUEST,
                    [("content-type", "application/json")],
                    serde_json::to_string(&resp).unwrap_or_default(),
                );
            }
        };

        // Find cache rule for the path
        let path_bytes = path_str.as_bytes();
        let rule = match match_cache_rule(&controller.cfg, path_bytes) {
            Ok(r) => Arc::new(r.clone()),
            Err(_) => {
                let resp = MarkedResponse {
                    success: false,
                    affected: 0,
                };
                return (
                    StatusCode::NOT_FOUND,
                    [("content-type", "application/json")],
                    serde_json::to_string(&resp).unwrap_or_default(),
                );
            }
        };

        // Build query string from remaining parameters (excluding _path)
        let mut query_parts = Vec::new();
        for (key, value) in &params {
            if key != PATH_SPECIAL && key != REMOVE_SPECIAL {
                query_parts.push(format!("{}={}", key, urlencoding::encode(value)));
            }
        }
        let query_str = if query_parts.is_empty() {
            String::new()
        } else {
            query_parts.join("&")
        };

        // Filter and sort query parameters according to rule
        let filtered_queries = filter_and_sort_request(Some(&*rule), &query_str);

        // Determine if we should remove entries (check for _remove query param)
        let should_remove = params.contains_key(REMOVE_SPECIAL);

        // Walk through all shards and invalidate matching entries
        let affected = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let keys_to_remove = Arc::new(std::sync::Mutex::new(Vec::new()));
        let affected_clone = affected.clone();
        let keys_to_remove_clone = keys_to_remove.clone();
        let db_clone = controller.db.clone();
        let rule_clone = rule.clone();
        let filtered_queries_clone = filtered_queries.clone();
        let path_bytes_clone = path_bytes.to_vec();

        let ctx = CancellationToken::new();
        db_clone.walk_shards(ctx.clone(), Box::new(move |_shard_id, shard| {
            let affected = affected_clone.clone();
            let keys_to_remove = keys_to_remove_clone.clone();
            let rule = rule_clone.clone();
            let filtered_queries = filtered_queries_clone.clone();
            let path_bytes = path_bytes_clone.clone();
            let should_remove = should_remove;

            shard.walk_r(&ctx, |key, entry| {
                // Check if path matches
                let entry_path = entry.rule.path_bytes.as_deref().unwrap_or(&[]);
                if entry_path != path_bytes {
                    return true; // Continue to next entry
                }

                // Check if rule path matches
                if entry.rule.path.as_deref() != rule.path.as_deref() {
                    return true; // Continue to next entry
                }

                // Unpack queries from entry payload
                let entry_queries = match entry.request_payload() {
                    Ok(payload) => payload.queries,
                    Err(_) => {
                        // If we can't unpack, skip this entry
                        return true;
                    }
                };

                // Check if queries match
                // If no filtered queries provided, match all entries for the path
                // Otherwise, all filtered queries must be present in entry with exact values
                let matches = if filtered_queries.is_empty() {
                    true // No query filter - match all entries for this path
                } else {
                    // All filtered queries must be present in entry with exact match
                    let mut all_match = true;
                    for (filter_key, filter_value) in &filtered_queries {
                        let mut found = false;
                        for (entry_key, entry_value) in &entry_queries {
                            if entry_key == filter_key && entry_value == filter_value {
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            all_match = false;
                            break;
                        }
                    }
                    all_match
                };

                if !matches {
                    return true; // Continue to next entry
                }

                // Match found - invalidate or remove
                if should_remove {
                    // Collect key for removal (we'll remove after walk completes)
                    keys_to_remove.lock().unwrap().push(key);
                    affected.fetch_add(1, Ordering::Relaxed);
                } else {
                    // Mark as outdated (set updated_at to past)
                    // Note: untouch_refreshed_at requires mutable access, but we have immutable
                    // We need to get the entry mutably or use a different approach
                    // For now, we'll collect keys and handle them separately
                    keys_to_remove.lock().unwrap().push(key);
                    affected.fetch_add(1, Ordering::Relaxed);
                }

                true // Continue to next entry
            });
        }));

        // Handle collected entries: mark as outdated or remove
        let keys = keys_to_remove.lock().unwrap().clone();
        for key in keys {
            if let (Some(entry), _) = controller.db.get_by_key(key) {
                if should_remove {
                    // Remove entry
                    controller.db.remove(&entry);
                } else {
                    // Mark as outdated - set updated_at to past (making entry expired)
                    // This is equivalent to untouch_refreshed_at()
                    let ttl_nanos = entry.rule.refresh.as_ref()
                        .and_then(|r| r.ttl)
                        .map(|d| d.as_nanos() as i64)
                        .unwrap_or(0);
                    let now = crate::time::unix_nano();
                    entry.updated_at.store(now - ttl_nanos, Ordering::Relaxed);
                }
            }
        }

        let affected_count = affected.load(Ordering::Relaxed);
        
        let resp = MarkedResponse {
            success: true,
            affected: affected_count,
        };
        
        tracing::info!(
            component = "invalidate",
            path = %path_str,
            affected = affected_count,
            removed = should_remove,
            "cache entries marked as outdated"
        );
        
        (
            StatusCode::OK,
            [("content-type", "application/json")],
            serde_json::to_string(&resp).unwrap_or_default(),
        )
    }
}

impl Controller for InvalidateController {
    fn add_route(&self, router: Router) -> Router {
        let controller = Arc::new(self.clone());
        router
            .route("/advcache/invalidate", get(move |query: Query<HashMap<String, String>>| {
                let controller = controller.clone();
                async move {
                    Self::invalidate(query, State(controller)).await
                }
            }))
    }
}

impl Clone for InvalidateController {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            cfg: self.cfg.clone(),
        }
    }
}