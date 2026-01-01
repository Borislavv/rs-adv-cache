// Main entrypoint for the AdvCache application.

mod app;
#[path = "shared/bytes/mod.rs"]
mod bytes;
mod config;
mod controller;
#[path = "shared/dedlog/mod.rs"]
mod dedlog;
mod governor;
mod http;
#[path = "k8s/probe/liveness/mod.rs"]
mod liveness;
mod metrics;
mod middleware;
mod model;
#[path = "shared/rand/mod.rs"]
mod rand;
#[path = "shared/rate/mod.rs"]
mod rate;
mod shutdown;
#[path = "shared/sort/mod.rs"]
mod sort;
mod db;
#[path = "shared/time/mod.rs"]
mod time;
mod traces;
mod upstream;
mod workers;

use crate::config::{Config, ConfigTrait};
use crate::shutdown::GracefulShutdown;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

const CONFIG_PATH: &str = "cfg/advcache.cfg.yaml";
const CONFIG_PATH_LOCAL: &str = "cfg/advcache.cfg.local.yaml";

/// AdvCache - High-performance in-memory HTTP cache & reverse proxy
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Custom config file path
    #[arg(short, long, value_name = "FILE")]
    cfg: Option<PathBuf>,
}

/// Configures and logs thread parallelism settings.
/// Tokio runtime automatically uses all available CPU cores.
fn set_max_num_cpus(cfg: &Config) {
    let cores = cfg.runtime().num_cpus;
    if cores == 0 {
        let cores = num_cpus::get();
        info!(
            component = "main",
            event = "num_cpus_configured",
            num_cpus = cores,
            "Available cores value configured (using all available cores)"
        );
    } else {
        warn!(
            component = "main",
            event = "num_cpus_configured",
            num_cpus = cores,
            "Available cores value configured"
        );
    }
}

/// Loads the configuration struct from YAML file.
/// Tries local config first, then falls back to default config.
fn load_cfg(path: Option<PathBuf>) -> Result<Config> {
    if let Some(custom_path) = path {
        let cfg = Config::load(&custom_path)
            .with_context(|| format!("failed to load custom config from {:?}", custom_path))?;
        info!(
            component = "config",
            event = "load_success",
            path = ?custom_path,
            "config loaded"
        );
        return Ok(cfg);
    }

    // Try local config first
    match Config::load(PathBuf::from(CONFIG_PATH_LOCAL)) {
        Ok(cfg) => {
            info!(
                component = "config",
                event = "load_success",
                path = CONFIG_PATH_LOCAL,
                "config loaded"
            );
            Ok(cfg)
        }
        Err(_) => {
            // Fall back to default config
            let cfg = Config::load(PathBuf::from(CONFIG_PATH))
                .with_context(|| format!("failed to load config from {}", CONFIG_PATH))?;
            info!(
                component = "config",
                event = "load_success",
                path = CONFIG_PATH,
                "config loaded"
            );
            Ok(cfg)
        }
    }
}

/// Configures structured logging based on configuration.
fn configure_logger(cfg: &Config) {
    use tracing_subscriber::fmt;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

    let log_level = cfg
        .logs()
        .and_then(|logs| logs.level.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("debug");

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

    if cfg.is_prod() {
        // Production: JSON format
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json())
            .init();
    } else {
        // Development: Pretty console format
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().pretty())
            .init();
    }
}

fn main() -> Result<()> {
    // Parse command-line arguments
    let args = Args::parse();
    
    // Initialize Prometheus metrics exporter BEFORE tokio runtime starts
    // This is critical to avoid "Cannot drop a runtime" errors
    match crate::controller::metrics::init_prometheus_exporter() {
        Ok(_) => {
            eprintln!("Info: Prometheus metrics exporter initialized successfully");
        }
        Err(e) => {
            eprintln!("Warning: Failed to initialize Prometheus metrics exporter: {}", e);
            eprintln!("Metrics endpoint will not be available");
        }
    }
    
    // Now start the async runtime
    tokio::runtime::Runtime::new()
        .context("Failed to create tokio runtime")?
        .block_on(async_main(args))
}

async fn async_main(args: Args) -> Result<()> {

    // Create cancellation token for graceful shutdown
    let shutdown_token = CancellationToken::new();

    // Start time caching to reduce syscalls
    let _ctime_token = time::start(Duration::from_millis(1));

    // Load configuration
    let cfg = load_cfg(args.cfg)?;

    // Configure logger (must be done after config is loaded)
    configure_logger(&cfg);

    // Optimize thread parallelism
    set_max_num_cpus(&cfg);

    // Start deduplicated error logger
    let dedup_logger_token = shutdown_token.clone();
    tokio::task::spawn(async move {
        dedlog::start_dedup_logger(dedup_logger_token).await;
    });

    // Setup graceful shutdown handler
    let graceful_shutdown = GracefulShutdown::new(shutdown_token.clone());
    graceful_shutdown
        .set_graceful_timeout(Duration::from_secs(60))
        .await;

    // Initialize liveness probe for Kubernetes/Cloud health checks
    let probe_timeout = cfg
        .k8s()
        .and_then(|k8s| k8s.probe.timeout)
        .unwrap_or(Duration::from_secs(5));
    let probe = Arc::new(liveness::Probe::new(probe_timeout)) as Arc<dyn liveness::Prober>;

    // Initialize and start the cache application
    let app = app::App::new(shutdown_token.clone(), cfg, probe).await?;

    // Register app for graceful shutdown
    graceful_shutdown.add(1);

    // Start the app in a background task
    let app_clone = app.clone();
    let graceful_done = Arc::new(graceful_shutdown.clone());
    tokio::task::spawn(async move {
        if let Err(e) = app_clone.serve(graceful_done.clone()).await {
            error!(
                component = "main",
                scope = "app",
                event = "start_failed",
                error = %e,
                "failed to start app"
            );
        }
        graceful_done.done();
    });

    // Listen for OS signals or cancellation and wait for graceful shutdown
    if let Err(e) = graceful_shutdown.await_shutdown().await {
        error!(
            component = "main",
            scope = "service",
            event = "graceful_shutdown_failed",
            error = %e,
            "failed to gracefully shut down service"
        );
        return Err(e);
    }

    Ok(())
}
