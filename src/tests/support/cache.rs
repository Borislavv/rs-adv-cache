// Cache server bootstrap for integration tests.
use crate::app::App;
use crate::config;
use crate::liveness;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Cache server wrapper for tests.
pub struct CacheServer {
    addr: String,
    shutdown_token: CancellationToken,
    stop_handle: tokio::task::JoinHandle<()>,
}

impl CacheServer {
    /// Starts the cache server.
    pub async fn start(
        shutdown_token: CancellationToken,
        cfg: config::Config,
        upstream_addr: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Update config to point to upstream
        let mut cfg = cfg;
        if let Some(ref mut upstream) = cfg.cache.upstream {
            if let Some(ref mut backend) = upstream.backend {
                backend.host = Some(upstream_addr.to_string());
                backend.host_bytes = Some(upstream_addr.as_bytes().to_vec());
            }
        }

        let probe =
            Arc::new(liveness::Probe::new(Duration::from_secs(1))) as Arc<dyn liveness::Prober>;
        let app = Arc::new(App::new(shutdown_token.clone(), cfg.clone(), probe.clone()).await?);

        let graceful_shutdown = Arc::new(crate::shutdown::GracefulShutdown::new(
            shutdown_token.clone(),
        ));
        graceful_shutdown.add(1);

        let app_clone = app.clone();
        let graceful_clone = graceful_shutdown.clone();
        let stop_handle = tokio::spawn(async move {
            if let Err(e) = app_clone.serve(graceful_clone).await {
                eprintln!("[cache] serve failed: {}", e);
            }
        });

        // Wait for cache to become alive by checking health endpoint
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let addr = format!(
            "127.0.0.1:{}",
            cfg.cache
                .api
                .as_ref()
                .and_then(|a| a.port.as_ref())
                .unwrap_or(&"8091".to_string())
        );
        let health_url = format!("http://{}/healthz", addr);

        while tokio::time::Instant::now() < deadline {
            if let Ok(resp) = reqwest::get(&health_url).await {
                if resp.status().is_success() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Final check
        if let Ok(resp) = reqwest::get(&health_url).await {
            if !resp.status().is_success() {
                return Err("timed out waiting for cache to become alive".into());
            }
        } else {
            return Err("timed out waiting for cache to become alive".into());
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        Ok(Self {
            addr,
            shutdown_token,
            stop_handle,
        })
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Stops the cache server.
    pub async fn stop(self) {
        self.shutdown_token.cancel();
        self.stop_handle.abort();
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
