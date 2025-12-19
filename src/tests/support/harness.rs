// Integration test harness setup.

use super::cache::CacheServer;
use super::upstream::UpstreamServer;
use crate::config;
use std::sync::OnceLock;
use tokio::sync::oneshot;
use tokio::sync::OnceCell;

// Global addresses for reuse across tests.
static UP_ADDR: OnceCell<String> = OnceCell::const_new();
static CACHE_ADDR: OnceCell<String> = OnceCell::const_new();
static STARTED: OnceLock<()> = OnceLock::new();

/// Initializes the test harness (upstream and cache servers).
/// This is called once before all tests.
pub async fn init_test_harness() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if STARTED.get().is_some() {
        return Ok(());
    }

    let (tx, rx) = oneshot::channel();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("init test runtime");
        rt.block_on(async move {
            let upstream = UpstreamServer::start().await;
            let up_addr = format!("http://{}", upstream.addr());
            println!("[e2e] upstream at {}/healthz", up_addr);

            let mut cfg = config::new_test_config();
            if let Some(ref mut upstream_cfg) = cfg.cache.upstream {
                if let Some(ref mut backend) = upstream_cfg.backend {
                    // Strip scheme for backend host
                    let host = up_addr.trim_start_matches("http://").to_string();
                    backend.host = Some(host.clone());
                    backend.host_bytes = Some(host.as_bytes().to_vec());
                }
            }

            let shutdown_token = tokio_util::sync::CancellationToken::new();
            let cache = CacheServer::start(
                shutdown_token.clone(),
                cfg,
                &up_addr.trim_start_matches("http://"),
            )
            .await
            .expect("cache start");
            let cache_addr = format!("http://{}", cache.addr());
            println!("[e2e] cache at {}/healthz", cache_addr);

            // Share addresses back to caller, ignore if receiver is gone.
            let _ = tx.send((up_addr.clone(), cache_addr.clone()));

            // Keep servers alive for the duration of the process.
            futures::future::pending::<()>().await;
        });
    });

    let (up_addr, cache_addr) = rx.await.map_err(|e| e.to_string())?;
    let _ = UP_ADDR.set(up_addr);
    let _ = CACHE_ADDR.set(cache_addr);
    let _ = STARTED.set(());
    Ok(())
}

/// Cleans up the test harness.
pub async fn cleanup_test_harness() {
    // No-op for now; background runtime lives for process lifetime.
}

/// Gets the cache server address.
pub async fn cache_addr() -> String {
    CACHE_ADDR
        .get()
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:8091".to_string())
}

/// Gets the upstream server address.
pub async fn upstream_addr() -> String {
    UP_ADDR.get().cloned().unwrap_or_default()
}
