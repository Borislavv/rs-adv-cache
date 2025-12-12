// Integration test harness setup.

use std::sync::OnceLock;
use tokio::sync::Mutex;

use super::upstream::UpstreamServer;
use super::cache::CacheServer;
use advcache::config;

/// Global test state (initialized once).
static UPSTREAM: OnceLock<Mutex<Option<UpstreamServer>>> = OnceLock::new();
static CACHE: OnceLock<Mutex<Option<CacheServer>>> = OnceLock::new();
static INIT: OnceLock<tokio::sync::Mutex<bool>> = OnceLock::new();

/// Initializes the test harness (upstream and cache servers).
/// This is called once before all tests.
pub async fn init_test_harness() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let init_mutex = INIT.get_or_init(|| tokio::sync::Mutex::new(false));
    let mut initialized = init_mutex.lock().await;
    
    if *initialized {
        return Ok(());
    }
    
    // 1) Start upstream once
    let upstream = UpstreamServer::start().await;
    println!("[e2e] upstream at http://{}/healthcheck", upstream.addr());
    
    UPSTREAM.get_or_init(|| Mutex::new(Some(upstream)));
    
    // 2) Build test config and point upstream backend to the upstream address
    let mut cfg = config::new_test_config();
    if cfg.cache.upstream.is_none() || cfg.cache.upstream.as_ref().unwrap().backend.is_none() {
        return Err("NewTestConfig is missing Upstream.Backend".into());
    }
    
    let upstream_addr = {
        let up_guard = UPSTREAM.get().unwrap().lock().await;
        up_guard.as_ref().unwrap().addr().to_string()
    };
    
    // 3) Start cache (one instance for whole test suite)
    let shutdown_token = tokio_util::sync::CancellationToken::new();
    let cache = CacheServer::start(shutdown_token.clone(), cfg, &upstream_addr).await?;
    println!("[e2e] cache at http://{}/healthz", cache.addr());
    
    CACHE.get_or_init(|| Mutex::new(Some(cache)));
    
    *initialized = true;
    Ok(())
}

/// Cleans up the test harness.
pub async fn cleanup_test_harness() {
    if let Some(cache_mutex) = CACHE.get() {
        if let Some(cache) = cache_mutex.lock().await.take() {
            cache.stop().await;
        }
    }
    
    if let Some(upstream_mutex) = UPSTREAM.get() {
        if let Some(upstream) = upstream_mutex.lock().await.take() {
            upstream.close().await;
        }
    }
}

/// Gets the cache server address.
pub async fn cache_addr() -> String {
    if let Some(cache_mutex) = CACHE.get() {
        if let Some(cache) = cache_mutex.lock().await.as_ref() {
            return format!("http://{}", cache.addr());
        }
    }
    "http://127.0.0.1:8091".to_string()
}

/// Gets the upstream server address.
pub async fn upstream_addr() -> String {
    if let Some(upstream_mutex) = UPSTREAM.get() {
        if let Some(upstream) = upstream_mutex.lock().await.as_ref() {
            return format!("http://{}", upstream.addr());
        }
    }
    String::new()
}

