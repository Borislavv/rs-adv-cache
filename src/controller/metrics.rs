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

        let metrics_text = match PrometheusBuilder::new().install() {
            Ok(_) => {
                metrics::describe_counter!(crate::metrics::TOTAL, "Total number of requests");
                "# Metrics available\n".to_string()
            }
            Err(_) => "# Metrics exporter not initialized\n".to_string(),
        };

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
