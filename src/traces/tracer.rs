use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::config::Traces;

// Constants for exporter types
const EXPORTER_GRPC: &str = "grpc"; // default

// Attribute key constants
#[allow(dead_code)]
pub const ATTR_HTTP_PATH_KEY: &str = "http.path";
#[allow(dead_code)]
pub const ATTR_HTTP_REQUEST: &str = "http.request";
pub const ATTR_HTTP_STATUS_CODE_KEY: &str = "http.status_code";
pub const ATTR_HTTP_RESPONSE_SIZE_KEY: &str = "http.response_size";
pub const ATTR_CACHE_PROXY: &str = "cache.proxy";
pub const ATTR_CACHE_HIT: &str = "cache.hit";
pub const ATTR_CACHE_KEY: &str = "cache.key";
pub const ATTR_CACHE_IS_ERR: &str = "cache.is_err";

// Global state
static SERVICE_NAME: Mutex<Option<String>> = Mutex::new(None);
static ENABLED: AtomicBool = AtomicBool::new(false);
static CUR_TP: Mutex<Option<Arc<opentelemetry_sdk::trace::TracerProvider>>> = Mutex::new(None);
static MU: Mutex<()> = Mutex::new(()); // Serialize Apply to avoid double-shutdown races

// Error types
#[derive(Debug, thiserror::Error)]
pub enum TracesError {
    #[error("endpoint is empty for selected exporter")]
    EndpointEmpty,
    #[error("service name is empty")]
    ServiceNameEmpty,
}

/// Checks if tracing is active.
pub fn is_active_tracing() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Enables tracing.
pub fn enable_tracing() {
    ENABLED.store(true, Ordering::Relaxed);
}

/// Disables tracing.
pub fn disable_tracing() {
    ENABLED.store(false, Ordering::Relaxed);
}

/// Applies tracing configuration and returns a shutdown function.
pub fn apply(
    _shutdown_token: CancellationToken,
    cfg: Option<Traces>,
) -> Box<dyn Fn(CancellationToken) -> Result<()> + Send + Sync> {
    let _guard = MU.lock().unwrap();

    let cfg = match cfg {
        Some(c) => c,
        None => {
            // Switch to a minimal provider with NeverSample sampler (fast noop)
            let _old = CUR_TP.lock().unwrap().take();
            ENABLED.store(false, Ordering::Relaxed);
            return Box::new(move |_| Ok(()));
        }
    };

    if !cfg.enabled {
        // Switch to a minimal provider with NeverSample sampler (fast noop)
        let _old = CUR_TP.lock().unwrap().take();
        ENABLED.store(false, Ordering::Relaxed);
        return Box::new(move |_| Ok(()));
    }

    // Validate configuration
    let _exporter = cfg.exporter.as_deref().unwrap_or(EXPORTER_GRPC);
    let _endpoint = match cfg.endpoint.as_ref() {
        Some(e) => e.clone(),
        None => {
            ENABLED.store(false, Ordering::Relaxed);
            return Box::new(move |_| Err(TracesError::EndpointEmpty.into()));
        }
    };

    let service_name = match cfg.service_name.as_ref() {
        Some(s) => s.clone(),
        None => {
            ENABLED.store(false, Ordering::Relaxed);
            return Box::new(move |_| Err(TracesError::ServiceNameEmpty.into()));
        }
    };

    // Configure service name
    *SERVICE_NAME.lock().unwrap() = Some(service_name.clone());

    ENABLED.store(true, Ordering::Relaxed);
    Box::new(move |_| {
        let _guard = MU.lock().unwrap();
        if let Some(provider) = CUR_TP.lock().unwrap().take() {
            // Shutdown the provider
            drop(provider); // Provider will shutdown on drop
        }
        ENABLED.store(false, Ordering::Relaxed);
        Ok(())
    })
}

/// Extracts trace context from incoming request headers.
/// Returns current context if tracing is disabled (fast no-op path).
pub fn extract(headers: &axum::http::HeaderMap) -> opentelemetry::Context {
    // Fast path: return current context if tracing is disabled (no-op)
    if !is_active_tracing() {
        return opentelemetry::Context::current();
    }

    use opentelemetry::global;

    let mut header_map = std::collections::HashMap::new();
    for (key, value) in headers.iter() {
        if let Ok(value_str) = value.to_str() {
            header_map.insert(key.as_str().to_string(), value_str.to_string());
        }
    }

    struct HeaderMapCarrier {
        headers: std::collections::HashMap<String, String>,
    }

    impl opentelemetry::propagation::Extractor for HeaderMapCarrier {
        fn get(&self, key: &str) -> Option<&str> {
            self.headers.get(key).map(|s| s.as_str())
        }

        fn keys(&self) -> Vec<&str> {
            self.headers.keys().map(|k| k.as_str()).collect()
        }
    }

    let carrier = HeaderMapCarrier {
        headers: header_map,
    };

    // Extract using the global propagator
    // Use a closure to extract context and return it
    let mut extracted_ctx = opentelemetry::Context::current();
    global::get_text_map_propagator(|propagator| {
        extracted_ctx = propagator.extract(&carrier);
    });
    extracted_ctx
}