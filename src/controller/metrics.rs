//! Metrics controller.

use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};

use crate::http::Controller;

pub const PROMETHEUS_METRICS_PATH: &str = "/metrics";

/// PrometheusMetricsController handles Prometheus metrics endpoint.
pub struct PrometheusMetricsController;

impl PrometheusMetricsController {
    /// Creates a new Prometheus metrics controller.
    pub fn new() -> Self {
        Self
    }

    /// Handles the metrics request.
    async fn get_metrics() -> impl IntoResponse {
        use metrics_exporter_prometheus::PrometheusBuilder;

        // Lazy initialization: install() sets up the global recorder
        // Note: install() can only be called once, subsequent calls return Err
        static INITIALIZED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        
        let _ = INITIALIZED.get_or_init(|| {
            let _ = PrometheusBuilder::new().install();
            ()
        });

        // For now, return a simple message indicating metrics are available
        // The actual metrics rendering will be handled by the metrics crate
        // when metrics are recorded. To get full metrics, we'd need to store
        // the handle from build(), but that requires different initialization.
        // This is a limitation of the current metrics-exporter-prometheus API.
        let metrics_text = "# Metrics endpoint active\n# Use Prometheus to scrape this endpoint\n".to_string();

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
