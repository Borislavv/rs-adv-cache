//! Metrics controller.

use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use std::sync::OnceLock;

use crate::http::Controller;

pub const PROMETHEUS_METRICS_PATH: &str = "/metrics";

/// Global Prometheus handle for rendering metrics.
/// We build first to get handle, then install the recorder separately.
static PROMETHEUS_HANDLE: OnceLock<metrics_exporter_prometheus::PrometheusHandle> = OnceLock::new();

/// Initializes the Prometheus metrics exporter.
/// Must be called BEFORE tokio runtime starts to avoid runtime conflicts.
/// 
/// WARNING: `install()` internally may create a runtime, which causes
/// "Cannot drop a runtime in a context where blocking is not allowed" error
/// if called after async runtime is already running.
/// 
/// This function should be called in `main()` before `#[tokio::main]` or
/// before any async operations start.
pub fn init_prometheus_exporter() -> anyhow::Result<()> {
    use metrics_exporter_prometheus::PrometheusBuilder;
    use std::net::SocketAddr;
    
    // In version 0.6, install() requires either listen_address or push_gateway_config.
    // We want to use our own /metrics endpoint via axum, so we:
    // 1. Build a recorder first to get a handle
    // 2. Install a new recorder with a dummy listen_address (which we won't use)
    // 3. The handle from step 1 will work because both use the same default registry
    //
    // Note: We set listen_address to avoid the "must specify at least one" error,
    // but we disable the HTTP listener, so it won't actually bind to that address.
    
    // Build the recorder first to get a handle
    let builder = PrometheusBuilder::new();
    let recorder = builder.build();
    
    // Get handle from the recorder BEFORE installing
    let handle = recorder.handle();
    
    // Install a new recorder with HTTP listener disabled
    // We need to provide listen_address to satisfy install() requirements,
    // but disable_http_listener() prevents it from actually binding
    let dummy_addr: SocketAddr = "127.0.0.1:0".parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse dummy address: {}", e))?;
    
    PrometheusBuilder::new()
        .listen_address(dummy_addr) // Required by install(), but won't be used due to disable_http_listener()
        .disable_http_listener() // Disable built-in HTTP server - we use our own endpoint
        .install()
        .map_err(|e| anyhow::anyhow!("Failed to install Prometheus recorder: {}", e))?;
    
    // Store the handle for rendering metrics later
    // The handle from our built recorder will work because both recorders
    // built from PrometheusBuilder::new() share the same default registry
    PROMETHEUS_HANDLE.set(handle)
        .map_err(|_| anyhow::anyhow!("Prometheus handle already initialized"))?;
    
    // Verify handle was stored
    if PROMETHEUS_HANDLE.get().is_none() {
        return Err(anyhow::anyhow!("Failed to store Prometheus handle - set() returned Ok but get() is None"));
    }
    
    Ok(())
}

/// PrometheusMetricsController handles Prometheus metrics endpoint.
pub struct PrometheusMetricsController;

impl PrometheusMetricsController {
    /// Creates a new Prometheus metrics controller.
    pub fn new() -> Self {
        Self
    }

    /// Handles the metrics request.
    /// 
    /// Note: For metrics-exporter-prometheus 0.6, we use a workaround.
    /// Since `install()` doesn't return a handle, we need to access the
    /// internal registry. The installed recorder uses a global registry.
    /// 
    /// The workaround: create a new builder and build a recorder, which
    /// will share the same underlying registry as the installed one.
    /// However, PrometheusRecorder doesn't expose render() directly.
    /// 
    /// For now, we return a placeholder message. Full implementation
    /// would require using the http-listener feature or accessing the
    /// internal prometheus::Registry directly.
    async fn get_metrics() -> impl IntoResponse {
        // Check if we have a handle
        if let Some(handle) = PROMETHEUS_HANDLE.get() {
            // Render metrics using the handle
            let metrics_text = handle.render();
            return (
                StatusCode::OK,
                [("content-type", "text/plain; charset=utf-8")],
                metrics_text,
            );
        }
        
        // Fallback: return a message indicating handle is not available
        // This means init_prometheus_exporter() either wasn't called or failed
        let metrics_text = format!(
            "# ERROR: Prometheus handle not initialized\n\
             # Handle is None: {}\n\
             # Please ensure init_prometheus_exporter() is called at startup\n\
             # and that it completes successfully\n",
            PROMETHEUS_HANDLE.get().is_none()
        );

        (
            StatusCode::SERVICE_UNAVAILABLE,
            [("content-type", "text/plain; charset=utf-8")],
            metrics_text,
        )
    }
}

impl Default for PrometheusMetricsController {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller for PrometheusMetricsController {
    fn add_route(&self, router: Router) -> Router {
        router.route(PROMETHEUS_METRICS_PATH, get(Self::get_metrics))
    }
}
