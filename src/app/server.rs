// HTTP server implementation for the cache application.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::error;

use crate::config::{Config, ConfigTrait};
use crate::http::{Controller, Middleware, Server as HttpServerTrait};
use crate::liveness;
use crate::governor::Governor;
use crate::storage::Storage;
use crate::upstream::Upstream;

/// HTTP server implementation that wraps all dependencies.
pub struct HttpServer {
    #[allow(dead_code)]
    ctx: CancellationToken,
    server: Arc<dyn HttpServerTrait>,
    is_server_alive: Arc<AtomicBool>,
}

impl HttpServer {
    /// Creates a new HttpServer, initializing metrics and the HTTP server.
    /// Returns an error if initialization fails.
    pub fn new(
        ctx: CancellationToken,
        cfg: Config,
        db: Arc<dyn Storage>,
        backend: Arc<dyn Upstream>,
        governor: Arc<dyn Governor>,
        probe: Arc<liveness::Probe>,
    ) -> Result<Self> {
        // Initialize HTTP server with all controllers and middlewares.
        let server = Self::make_http_server(ctx.clone(), &cfg, db.clone(), backend.clone(), governor.clone(), probe.clone())?;

        Ok(Self {
            ctx,
            server,
            is_server_alive: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Returns true if the server is marked as alive.
    pub fn is_alive(&self) -> bool {
        self.is_server_alive.load(Ordering::Relaxed)
    }

    /// Starts the HTTP server (blocking call).
    pub async fn listen_and_serve(&self) -> Result<()> {
        self.is_server_alive.store(true, Ordering::Relaxed);
        
        // Start server in a way that we can track its lifecycle
        let result = self.server.listen_and_serve().await;
        
        self.is_server_alive.store(false, Ordering::Relaxed);
        result
    }

    /// Closes the HTTP server.
    #[allow(dead_code)]
    pub fn close(&self) -> Result<()> {
        self.ctx.cancel();
        Ok(())
    }

    /// Creates the HTTP server instance with controllers and middlewares.
    fn make_http_server(
        ctx: CancellationToken,
        cfg: &Config,
        db: Arc<dyn Storage>,
        backend: Arc<dyn Upstream>,
        governor: Arc<dyn Governor>,
        probe: Arc<liveness::Probe>,
    ) -> Result<Arc<dyn HttpServerTrait>> {
        let controllers = Self::controllers(ctx.clone(), cfg, db.clone(), backend.clone(), governor.clone(), probe.clone());
        let middlewares = Self::middlewares(cfg);

        // Compose server with controllers and middlewares.
        let server = crate::http::HttpServer::new(ctx, cfg.clone(), controllers, middlewares)?;
        Ok(Arc::new(server))
    }

    /// Returns all HTTP controllers for the server.
    fn controllers(
        ctx: CancellationToken,
        cfg: &Config,
        db: Arc<dyn Storage>,
        backend: Arc<dyn Upstream>,
        governor: Arc<dyn Governor>,
        probe: Arc<liveness::Probe>,
    ) -> Vec<Box<dyn Controller>> {
        use crate::controller;
        
        vec![
            // Healthcheck probe endpoint
            Box::new(controller::LivenessProbeController::new(probe.clone())),
            // Metrics endpoint
            Box::new(controller::PrometheusMetricsController::new()),
            // Cache on/off switcher
            Box::new(controller::BypassOnOffController::new(cfg.clone())),
            // Clears cache
            Box::new(controller::ClearController::new(cfg.clone(), db.clone())),
            // Main cache handler
            Box::new(controller::CacheProxyController::new(ctx, cfg.clone(), db.clone(), backend.clone())),
            // Searches items by query and mark them as outdated
            Box::new(controller::InvalidateController::new(cfg.clone(), db.clone())),
            // Changes await/deny policy to upstream switcher
            Box::new(controller::ChangeBackendPolicyController::new()),
            // Switches between enable/disable for http compression middleware
            Box::new(controller::HttpCompressionController::new()),
            // Encodes and shows current config as json
            Box::new(controller::ShowConfigController::new(cfg.clone())),
            // Provides endpoints for manipulate of Refresher/Remover worker settings
            Box::new(controller::LifetimeManagerController::new(cfg.clone(), governor.clone())),
            // Provides endpoints for manipulate of Evictor worker settings
            Box::new(controller::EvictionController::new(governor.clone())),
            // Provides access to switch off/on admission control
            Box::new(controller::AdmissionController::new(cfg.clone())),
            // Provides access to enable/disable of open-telemetry traces
            Box::new(controller::TracesController::new()),
            // Provides access to single cache item by key
            Box::new(controller::GetController::new(db.clone())),
        ]
    }

    /// Returns the request middlewares for the server, executed in reverse order.
    fn middlewares(cfg: &Config) -> Vec<Box<dyn Middleware>> {
        vec![
            // Exec first - panic recovery
            Box::new(crate::middleware::recover_middleware::PanicRecoverMiddleware::new()),
            // Exec second - compression
            Box::new(crate::middleware::compression_middleware::CompressionMiddleware::new(
                cfg.compression().cloned()
            )),
        ]
    }
}