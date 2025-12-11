// Main cache application implementation.

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::config::{Config, ConfigTrait};
use crate::liveness;
use crate::traces;
use crate::governor;
use crate::storage;
use crate::upstream;

use super::server::HttpServer;

/// Encapsulates the entire cache application state.
pub struct App {
    cfg: Config,
    shutdown_token: CancellationToken,
    backend: Arc<dyn upstream::Upstream>,
    storage: Arc<dyn storage::Storage>,
    probe: Arc<liveness::Probe>,
    #[allow(dead_code)]
    cancel_observer: Option<Arc<dyn Fn(CancellationToken) -> Result<()> + Send + Sync>>,
    server: Arc<HttpServer>,
}

impl App {
    /// Creates a new cache application instance.
    pub async fn new(
        shutdown_token: CancellationToken,
        cfg: Config,
        probe: liveness::Probe,
    ) -> Result<Self> {
        let gov = Arc::new(governor::Orchestrator::new());
        let backend = upstream::BackendImpl::new(
            shutdown_token.clone(),
            cfg.upstream().and_then(|u| u.backend.as_ref()).cloned(),
            cfg.traces().cloned(),
        )?;
        let adv_cache = storage::DB::new(
            shutdown_token.clone(),
            cfg.clone(),
            gov.clone(),
            backend.clone(),
        )?;
        let probe_arc = Arc::new(probe);
        let http_server = Arc::new(HttpServer::new(
            shutdown_token.clone(),
            cfg.clone(),
            adv_cache.clone(),
            backend.clone(),
            gov.clone(),
            probe_arc.clone(),
        )?);
        let cancel_observer = traces::apply(shutdown_token.clone(), cfg.traces().cloned());
        let cancel_observer_arc = Arc::new(cancel_observer);

        Ok(Self {
            cfg,
            shutdown_token,
            probe: probe_arc,
            storage: adv_cache,
            server: http_server,
            backend,
            cancel_observer: Some(cancel_observer_arc),
        })
    }

    /// Serves the cache server and probes, handles graceful shutdown.
    pub async fn serve(&self, gsh: Arc<crate::shutdown::GracefulShutdown>) -> Result<()> {
        let server = self.server.clone();
        let _probe = self.probe.clone();
        let _app_service = Arc::new(AppService {
            server: self.server.clone(),
        });

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
    #[allow(dead_code)]
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
}

impl Clone for App {
    fn clone(&self) -> Self {
        Self {
            cfg: self.cfg.clone(),
            shutdown_token: self.shutdown_token.clone(),
            backend: self.backend.clone(),
            storage: self.storage.clone(),
            probe: self.probe.clone(),
            cancel_observer: None, // Skip cloning the observer function
            server: self.server.clone(),
        }
    }
}

/// AppService implements liveness::Service for the App
struct AppService {
    #[allow(dead_code)] // Used in is_alive() method
    server: Arc<HttpServer>,
}

impl liveness::Service for AppService {
    fn is_alive(&self, _timeout: Duration) -> bool {
        self.server.is_alive()
    }
}

