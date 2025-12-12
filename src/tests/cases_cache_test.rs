// Integration tests for cache functionality.

#[path = "support/mod.rs"]
mod support;

use std::collections::HashMap;
use support::{assert_equal, assert_ok, do_json, new_namespace, with_ns, H, cache_addr, init_test_harness};

#[tokio::test]
async fn test_cache_hit_on_warm_identity() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_cache_hit_on_warm_identity");
    let base = cache_addr().await;
    
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "42".to_string());
    params.insert("domain".to_string(), "example.org".to_string());
    params.insert("language".to_string(), "en".to_string());
    
    let path = with_ns("/api/v1/user", &ns, &params);
    
    // Warm
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (status, _, _, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await);
    assert_equal(200, status);
    
    // Hit
    let (status, _, _, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await);
    assert_equal(200, status);
}

#[tokio::test]
async fn test_cache_vary_by_accept_encoding() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_cache_vary_by_accept_encoding");
    let base = cache_addr().await;
    
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "42".to_string());
    params.insert("domain".to_string(), "example.org".to_string());
    params.insert("language".to_string(), "en".to_string());
    
    let path = with_ns("/api/v1/user", &ns, &params);
    
    // identity x2
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let _ = do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await;
    let _ = do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await;
    
    // gzip x2
    headers.insert("Accept-Encoding".to_string(), "gzip".to_string());
    let _ = do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await;
    let _ = do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await;
    
    // Smoke: both AEs must be 200
    for ae in &["identity", "gzip"] {
        headers.insert("Accept-Encoding".to_string(), ae.to_string());
        let (status, _, _, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await);
        assert_equal(200, status);
    }
}

#[tokio::test]
async fn test_proxy_propagates_5xx_no_pollution() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_proxy_propagates_5xx_no_pollution");
    let base = cache_addr().await;
    
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "77".to_string());
    params.insert("domain".to_string(), "err.example".to_string());
    params.insert("language".to_string(), "en".to_string());
    
    let path = with_ns("/api/v1/user", &ns, &params);
    
    // Request non-existent path to get error status
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (status, _, _, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}/api/v1/not-exist?ns={}", base, ns), &headers).await);
    assert!(status >= 400, "expected error status, got {}", status);
    
    // Ensure normal endpoint still 200 (no cache pollution by previous error path)
    let (status, _, _, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await);
    assert_equal(200, status);
}

#[tokio::test]
async fn test_concurrency_cold_key_storm() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_concurrency_cold_key_storm");
    let base = cache_addr().await;
    
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "13".to_string());
    params.insert("domain".to_string(), "storm.example".to_string());
    params.insert("language".to_string(), "en".to_string());
    
    let path = with_ns("/api/v1/user", &ns, &params);
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    
    const N: usize = 24;
    let mut handles = Vec::new();
    for _ in 0..N {
        let base_clone = base.clone();
        let path_clone = path.clone();
        let headers_clone = headers.clone();
        let handle = tokio::spawn(async move {
            do_json::<serde_json::Value>("GET", &format!("{}{}", base_clone, path_clone), &headers_clone).await
        });
        handles.push(handle);
    }
    
    for handle in handles {
        let result = handle.await.unwrap();
        assert_ok(result);
    }
}

#[tokio::test]
async fn test_cache_serve_stale_during_refresh_soft() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_cache_serve_stale_during_refresh_soft");
    let base = cache_addr().await;
    
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "7".to_string());
    params.insert("domain".to_string(), "refresh.example".to_string());
    params.insert("language".to_string(), "en".to_string());
    
    let path = with_ns("/api/v1/user", &ns, &params);
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    
    // Warm
    let _ = do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await;
    
    // Wait a bit, then hit again quickly
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    let (status, _, _, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await);
    assert_equal(200, status);
}
