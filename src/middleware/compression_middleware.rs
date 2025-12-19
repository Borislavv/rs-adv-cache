//! Compression middleware.
//

use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use std::sync::atomic::{AtomicBool, Ordering};
use tower_http::compression::CompressionLayer;

use crate::config::Compression;

const APPLICATION_JSON: &str = "application/json";

/// Global compression enabled flag.
static IS_COMPRESSION_ENABLED: AtomicBool = AtomicBool::new(false);

/// Checks if compression is enabled.
pub fn is_compression_enabled() -> bool {
    IS_COMPRESSION_ENABLED.load(Ordering::Relaxed)
}

/// Enables compression.
pub fn enable_compression() {
    IS_COMPRESSION_ENABLED.store(true, Ordering::Relaxed);
}

/// Disables compression.
pub fn disable_compression() {
    IS_COMPRESSION_ENABLED.store(false, Ordering::Relaxed);
}

/// CompressionMiddleware provides HTTP response compression.
pub struct CompressionMiddleware {
    cfg: Option<Compression>,
}

impl CompressionMiddleware {
    /// Creates a new compression middleware.
    pub fn new(cfg: Option<Compression>) -> Self {
        if let Some(ref c) = cfg {
            if c.enabled {
                enable_compression();
            }
        }
        Self { cfg }
    }

    /// Middleware function that applies compression.
    pub async fn middleware(&self, request: Request, next: Next) -> Response {
        let mut response = next.run(request).await;

        // If compression is enabled, it's handled by the CompressionLayer
        // We just need to ensure Content-Type is set if missing
        if response.headers().get("content-type").is_none() {
            let header_value = HeaderValue::from_static(APPLICATION_JSON);
            response.headers_mut().insert("content-type", header_value);
        }

        response
    }
}

impl Default for CompressionMiddleware {
    fn default() -> Self {
        Self::new(None)
    }
}

// Implementation of Middleware trait
impl crate::middleware::middleware::Middleware for CompressionMiddleware {
    fn apply(&self, router: axum::Router) -> axum::Router {
        let router = if is_compression_enabled() {
            // Apply compression layer based on config level
            let level = self.cfg.as_ref().and_then(|c| c.level).unwrap_or(1);

            // Convert compression level (0-9) to tower-http's CompressionLevel
            // tower-http uses CompressionLevel enum, where 0 = no compression (skip layer), 1-9 = compression levels
            if level > 0 {
                let compression_level = match level {
                    1..=5 => tower_http::compression::CompressionLevel::Fastest,
                    6..=8 => tower_http::compression::CompressionLevel::Default,
                    9 => tower_http::compression::CompressionLevel::Best,
                    _ => tower_http::compression::CompressionLevel::Default,
                };

                let layer = CompressionLayer::new().no_br().quality(compression_level);
                router.layer(layer)
            } else {
                router
            }
        } else {
            router
        };

        // Add the middleware function for Content-Type handling
        let cfg_clone = self.cfg.clone();
        router.layer(axum::middleware::from_fn(
            move |request: Request, next: Next| {
                let cfg = cfg_clone.clone();
                async move {
                    CompressionMiddleware::new(cfg)
                        .middleware(request, next)
                        .await
                }
            },
        ))
    }
}
