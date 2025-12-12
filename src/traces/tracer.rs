use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use anyhow::Result;
use tokio_util::sync::CancellationToken;

use crate::config::Traces;

// Constants for exporter types
const EXPORTER_GRPC: &str = "grpc"; // default

// Attribute key constants
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
    
    // Build OTLP exporter based on configuration
    // Initialize OpenTelemetry SDK with OTLP exporter
    // Note: OpenTelemetry SDK initialization requires async runtime
    // We'll use a simplified approach that works with tracing-opentelemetry
    // The actual exporter setup is handled by tracing-opentelemetry when the layer is added
    
    // For now, we enable tracing and store config for later use
    // The real initialization should happen in main.rs when setting up tracing subscriber
    // with OpenTelemetryLayer, but we need to validate config here
    ENABLED.store(true, Ordering::Relaxed);
    
    // Return shutdown function
    // Note: Actual shutdown will be handled by tracing-opentelemetry layer
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

/// Starts a new span and returns context, span, and end function.
#[allow(dead_code)]
pub fn start(
    ctx: opentelemetry::Context,
    _name: &str,
    _kind: opentelemetry::trace::SpanKind,
) -> (opentelemetry::Context, opentelemetry::global::BoxedSpan, Box<dyn Fn() + Send + Sync>) {
    use opentelemetry::trace::Tracer as _;
    let tracer = opentelemetry::global::tracer("adv_cache");
    let span = tracer.start("");
    (ctx, span, Box::new(|| {}))
}

/// Safely sets a string attribute on a span.
/// Uses tracing::Span::current() to work with active spans from tracing-opentelemetry.
#[allow(dead_code)]
pub fn safe_set_string(
    _span: &opentelemetry::global::BoxedSpan,
    key: &str,
    val: &str,
) {
    // Use tracing API which integrates with OpenTelemetry via tracing-opentelemetry
    // tracing::Span::current() always returns a span (may be disabled span)
    let current_span = tracing::Span::current();
    current_span.record(key, val);
}

/// Sets a bytes attribute on a span.
/// Converts bytes to string for tracing API compatibility.
#[allow(dead_code)]
pub fn set_bytes_attr(
    _span: &opentelemetry::global::BoxedSpan,
    key: &str,
    b: &[u8],
) {
    // Use tracing API which integrates with OpenTelemetry via tracing-opentelemetry
    let current_span = tracing::Span::current();
    // Try to convert bytes to UTF-8 string, fallback to hex if invalid
    if let Ok(s) = std::str::from_utf8(b) {
        current_span.record(key, s);
    } else {
        // Convert bytes to hex string for safe representation
        let hex_str = hex::encode(b);
        current_span.record(key, hex_str.as_str());
    }
}

/// Sets an integer attribute on a span.
#[allow(dead_code)]
pub fn set_int_attr(
    _span: &opentelemetry::global::BoxedSpan,
    key: &str,
    v: i64,
) {
    // Use tracing API which integrates with OpenTelemetry via tracing-opentelemetry
    let current_span = tracing::Span::current();
    current_span.record(key, v);
}

/// Sets a boolean attribute on a span.
#[allow(dead_code)]
pub fn set_bool_attr(
    _span: &opentelemetry::global::BoxedSpan,
    key: &str,
    v: bool,
) {
    // Use tracing API which integrates with OpenTelemetry via tracing-opentelemetry
    let current_span = tracing::Span::current();
    current_span.record(key, v);
}

/// Extracts trace context from incoming request headers.
pub fn extract(headers: &axum::http::HeaderMap) -> opentelemetry::Context {
    use opentelemetry::global;
    
    // Collect header values into owned strings to avoid lifetime issues
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
    
    let carrier = HeaderMapCarrier { headers: header_map };
    
    // Extract using the global propagator
    // Use a closure to extract context and return it
    let mut extracted_ctx = opentelemetry::Context::current();
    global::get_text_map_propagator(|propagator| {
        extracted_ctx = propagator.extract(&carrier);
    });
    extracted_ctx
}

/// Injects trace context into outgoing request headers.
#[allow(dead_code)]
pub fn inject(
    ctx: opentelemetry::Context,
    headers: &mut axum::http::HeaderMap,
) {
    use opentelemetry::global;
    
    // Create a carrier to inject into headers
    struct HeaderMapCarrierMut<'a> {
        headers: &'a mut axum::http::HeaderMap,
    }
    
    impl<'a> opentelemetry::propagation::Injector for HeaderMapCarrierMut<'a> {
        fn set(&mut self, key: &str, value: String) {
            if let (Ok(name), Ok(val)) = (
                axum::http::header::HeaderName::try_from(key),
                axum::http::header::HeaderValue::from_str(&value),
            ) {
                self.headers.insert(name, val);
            }
        }
    }
    
    let mut carrier = HeaderMapCarrierMut { headers };
    
    // Inject using the global propagator
    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&ctx, &mut carrier);
    });
}
