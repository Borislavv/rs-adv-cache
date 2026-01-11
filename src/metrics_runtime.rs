use once_cell::sync::OnceCell;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use metrics_process::Collector;

static PROM_HANDLE: OnceCell<PrometheusHandle> = OnceCell::new();
static PROC_COLLECTOR: OnceCell<Collector> = OnceCell::new();

pub fn init_metrics() {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");
    let _ = PROM_HANDLE.set(handle);

    let collector = Collector::default();
    collector.describe(); // регистрирует HELP/TYPE
    let _ = PROC_COLLECTOR.set(collector);
}

pub fn scrape_prometheus_text() -> Option<String> {
    let h = PROM_HANDLE.get()?;
    if let Some(c) = PROC_COLLECTOR.get() {
        c.collect(); // <-- КЛЮЧЕВО
    }
    Some(h.render())
}

/// Runs upkeep for histogram housekeeping.
/// In metrics-exporter-prometheus 0.17, upkeep is available and should be called periodically
/// for histogram housekeeping. For process_* metrics, collector.collect() is what matters,
/// which is already called in scrape_prometheus_text() before rendering.
pub fn run_upkeep_periodically() {
    if let Some(h) = PROM_HANDLE.get() {
        h.run_upkeep();
    }
}
