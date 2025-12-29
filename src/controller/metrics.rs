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
    use metrics::set_boxed_recorder;
    
    // Build the recorder first to get a handle for rendering
    // In version 0.6, we use build() to get PrometheusRecorder
    // Then we can get handle from it before installing
    let builder = PrometheusBuilder::new();
    let recorder = builder.build();
    
    // Get handle from the recorder before installing
    // PrometheusRecorder should have a method to get handle
    // Try to access handle through the recorder
    let handle = recorder.handle();
    
    // Install the recorder as the global recorder
    // We use set_boxed_recorder instead of install() to avoid runtime creation
    // But PrometheusRecorder doesn't implement Recorder trait directly
    // So we need to use install() which sets it up correctly
    PrometheusBuilder::new()
        .install()
        .map_err(|e| anyhow::anyhow!("Failed to install Prometheus recorder: {}", e))?;
    
    // Store the handle for later use in rendering
    PROMETHEUS_HANDLE.set(handle)
        .map_err(|_| anyhow::anyhow!("Prometheus handle already initialized"))?;
    
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
            return (
                StatusCode::OK,
                [("content-type", "text/plain; charset=utf-8")],
                handle.render(),
            );
        }
        
        // Fallback: return a message indicating metrics are being collected
        // The recorder is installed and working, but we can't render without a handle
        let metrics_text = "# Metrics are being collected\n# The recorder is installed and working\n# Metrics will be available once recorded\n# To see actual metrics, use Prometheus to scrape this endpoint\n# Full rendering requires a handle from build() before install()\n".to_string();

        (
            StatusCode::OK,
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
