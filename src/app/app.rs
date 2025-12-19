// Main cache application implementation.

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::config::{Config, ConfigTrait};
use crate::governor;
use crate::liveness;
use crate::db;
use crate::traces;
use crate::upstream;

use super::server::{Http, HttpServer};

/// Encapsulates the entire cache application state.
pub struct App {
    cfg: Config,
    shutdown_token: CancellationToken,
    backend: Arc<dyn upstream::Upstream>,
    storage: Arc<dyn db::Storage>,
    probe: Arc<dyn liveness::Prober>,
    cancel_observer: Option<Arc<dyn Fn(CancellationToken) -> Result<()> + Send + Sync>>,
    server: Arc<dyn Http>,
}

impl App {
    /// Creates a new cache application instance.
    pub async fn new(
        shutdown_token: CancellationToken,
        cfg: Config,
        probe: Arc<dyn liveness::Prober>,
    ) -> Result<Self> {
        let gov = Arc::new(governor::Orchestrator::new());
        let backend = upstream::BackendImpl::new(
            shutdown_token.clone(),
            cfg.upstream().and_then(|u| u.backend.as_ref()).cloned(),
        )?;
        let adv_cache = db::DB::new(
            shutdown_token.clone(),
            cfg.clone(),
            gov.clone(),
            backend.clone(),
        )?;
        let http_server = Arc::new(HttpServer::new(
            shutdown_token.clone(),
            cfg.clone(),
            adv_cache.clone(),
            backend.clone(),
            gov.clone(),
            probe.clone(),
        )?);
        let cancel_observer = traces::apply(shutdown_token.clone(), cfg.traces().cloned());
        let cancel_observer_arc = Arc::new(cancel_observer);


        Ok(Self {
            cfg,
            shutdown_token,
            probe,
            storage: adv_cache,
            server: http_server,
            backend,
            cancel_observer: Some(cancel_observer_arc),
        })
    }

    /// Serves the cache server and probes, handles graceful shutdown.
    pub async fn serve(&self, gsh: Arc<crate::shutdown::GracefulShutdown>) -> Result<()> {
        // Register liveness target before serving.
        self.probe
            .watch(vec![Arc::new(self.clone()) as Arc<dyn liveness::Service>]);

        let server = self.server.clone();
        let app_for_close = self.clone();

        // Start probe watcher and server in background
        let gsh_clone = gsh.clone();

        tokio::task::spawn(async move {
            // Start server
            if let Err(e) = server.listen_and_serve().await {
                error!(
                    component = "app",
                    scope = "server",
                    event = "serve_failed",
                    error = %e,
                    "server failed to serve"
                );
            }

            if let Err(e) = app_for_close.close().await {
                error!(
                    component = "app",
                    scope = "shutdown",
                    event = "close_failed",
                    error = %e,
                    "application close failed"
                );
            }

            // Signal graceful shutdown
            gsh_clone.done();
        });

        info!(
            component = "app",
            event = "started",
            "application lifecycle"
        );

        Ok(())
    }

    /// Checks whether the HTTP server is still alive.
    pub fn is_alive(&self) -> bool {
        if !self.server.is_alive() {
            warn!(
                component = "app",
                scope = "http_server",
                event = "gone_away",
                "http server has gone away"
            );
            return false;
        }
        true
    }

    /// Closes application resources.
    pub async fn close(&self) -> Result<()> {
        if let Some(cb) = &self.cancel_observer {
            if let Err(e) = cb.as_ref()(self.shutdown_token.clone()) {
                error!(
                    component = "app",
                    scope = "observability",
                    event = "close_failed",
                    error = %e,
                    "error closing observer"
                );
            }
        }

        if let Err(e) = self.storage.close().await {
            error!(
                component = "app",
                scope = "storage",
                event = "close_failed",
                error = %e,
                "error closing storage"
            );
        }

        self.shutdown_token.cancel();

        info!(
            component = "app",
            event = "stopped",
            "application lifecycle"
        );

        Ok(())
    }
}

impl Clone for App {
    fn clone(&self) -> Self {
        Self {
            cfg: self.cfg.clone(),
            shutdown_token: self.shutdown_token.clone(),
            backend: self.backend.clone(),
            storage: self.storage.clone(),
            probe: self.probe.clone(),
            cancel_observer: self.cancel_observer.clone(),
            server: self.server.clone(),
        }
    }
}

/// AppService implements liveness::Service for the App
impl liveness::Service for App {
    fn is_alive(&self, _timeout: Duration) -> bool {
        self.is_alive()
    }
}
