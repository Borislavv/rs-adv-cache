//! HTTP server implementation.
//

use anyhow::{Context, Result};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower_http::timeout::TimeoutLayer;
use tracing::{error, info};

use crate::config::{Config, ConfigTrait};
use crate::controller::controller::Controller;
use crate::middleware::middleware::Middleware;

/// Server trait for HTTP server operations.
#[async_trait::async_trait]
pub trait Server: Send + Sync {
    /// Starts the server (blocking).
    async fn listen_and_serve(&self) -> Result<()>;
}

/// HTTP server implementation.
pub struct HttpServer {
    shutdown_token: CancellationToken,
    config: Config,
    router: Router,
}

impl HttpServer {
    /// Creates a new HTTP server.
    pub fn new(
        shutdown_token: CancellationToken,
        config: Config,
        controllers: Vec<Box<dyn Controller>>,
        middlewares: Vec<Box<dyn Middleware>>,
    ) -> Result<Arc<Self>> {
        let router = Self::build_router(controllers);
        let router = Self::merge_middlewares(router, middlewares);

        Ok(Arc::new(Self {
            shutdown_token,
            config,
            router,
        }))
    }

    /// Starts the HTTP server (async version).
    pub async fn listen_and_serve(&self) -> Result<()> {
        let api_cfg = self.config.api().context("API configuration is required")?;

        let name = api_cfg.name.as_deref().unwrap_or("advcache");
        let port = api_cfg.port.as_deref().unwrap_or("8020");

        // Ensure port starts with ':'
        let port = if port.starts_with(':') {
            port.to_string()
        } else {
            format!(":{}", port)
        };

        let addr: SocketAddr = format!("0.0.0.0{}", port)
            .parse()
            .context("Failed to parse server address")?;

        info!(
            component = "server",
            event = "started",
            name = name,
            port = port,
            "server started"
        );

        // Create TCP listener
        let listener = TcpListener::bind(&addr)
            .await
            .context("Failed to bind TCP listener")?;

        // Create shutdown signal
        let shutdown_token = self.shutdown_token.clone();

        // Start server with graceful shutdown
        let serve_future =
            axum::serve(listener, self.router.clone()).with_graceful_shutdown(async move {
                shutdown_token.cancelled().await;
            });

        // Run server
        if let Err(e) = serve_future.await {
            error!(
                component = "server",
                event = "listen_and_serve_failed",
                name = name,
                port = port,
                error = %e,
                "server failed to listen and serve"
            );
            return Err(e.into());
        }

        info!(
            component = "server",
            event = "stopped",
            name = name,
            port = port,
            "server stopped"
        );

        Ok(())
    }

    /// Builds the router with all controllers.
    fn build_router(controllers: Vec<Box<dyn Controller>>) -> Router {
        let mut router = Router::new();

        // Add routes from all controllers
        for controller in controllers {
            router = controller.add_route(router);
        }

        router
    }

    /// Merges middlewares into the router.
    fn merge_middlewares(router: Router, middlewares: Vec<Box<dyn Middleware>>) -> Router {
        let mut result = router;

        // Apply middlewares in reverse order (last middleware wraps first)
        for middleware in middlewares.iter().rev() {
            result = middleware.apply(result);
        }

        // Add timeout layer
        result = result.layer(TimeoutLayer::new(Duration::from_secs(30)));

        result
    }
}

#[async_trait::async_trait]
impl Server for HttpServer {
    async fn listen_and_serve(&self) -> Result<()> {
        // Delegate to the struct's async method
        HttpServer::listen_and_serve(self).await
    }
}

#[async_trait::async_trait]
impl Server for Arc<HttpServer> {
    async fn listen_and_serve(&self) -> Result<()> {
        HttpServer::listen_and_serve(self).await
    }
}
