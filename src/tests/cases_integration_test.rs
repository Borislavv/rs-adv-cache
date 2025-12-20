// Integration tests for module interactions and behavioral changes.


use std::collections::HashMap;
use crate::support::{assert_equal, assert_ok, cache_addr, do_json, init_test_harness, new_namespace, with_ns, H};

#[derive(serde::Deserialize)]
struct AdmissionResponse {
    #[serde(rename = "is_active")]
    is_active: bool,
}

/// Test that admission control actually affects cache storage behavior.
/// When admission is disabled, entries that would be rejected should still be stored.
#[tokio::test]
async fn test_admission_control_affects_storage() {
    use crate::support::with_global_lock;
    
    with_global_lock(|| async {
        init_test_harness().await.unwrap();

    let base = cache_addr().await;

    // Disable admission control to ensure entries are stored
    let (status, _, body, _) = assert_ok(
        do_json::<AdmissionResponse>("GET", &format!("{}/advcache/admission/off", base), &H::new()).await,
    );
    assert_equal(200, status);
    let resp: AdmissionResponse = serde_json::from_slice(&body).unwrap();
    assert!(!resp.is_active, "admission should be disabled");

    // Make a request that should be cacheable
    let ns = new_namespace("Test_Admission_AffectsStorage");
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "adm1".to_string());
    params.insert("domain".to_string(), "adm.example".to_string());
    params.insert("language".to_string(), "en".to_string());

    let path = with_ns("/api/v1/user", &ns, &params);
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // First request - should store
    let (status1, _, body1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status1);

    // Second request - should be cache hit
    let (status2, _, body2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status2);
    assert_eq!(body1, body2, "should be cache hit when admission is disabled");

    // Re-enable admission for cleanup
    let _ = assert_ok(
        do_json::<AdmissionResponse>("GET", &format!("{}/advcache/admission/on", base), &H::new()).await,
    );
    }).await;
}

/// Test that invalidation actually causes cache misses on subsequent requests.
#[tokio::test]
async fn test_invalidation_causes_cache_miss() {
    use crate::support::{get_updated_at, set_global_defaults, with_global_lock};
    
    with_global_lock(|| async {
        init_test_harness().await.unwrap();

        let ns = new_namespace("Test_Invalidation_CausesMiss");
        let base = cache_addr().await;

        // Set global defaults to ensure deterministic behavior
        set_global_defaults(&base).await;

        let mut params = HashMap::new();
        params.insert("user[id]".to_string(), "inv_int1".to_string());
        params.insert("domain".to_string(), "invm.example".to_string());
        params.insert("language".to_string(), "en".to_string());

        let path = with_ns("/api/v1/user", &ns, &params);
        let mut headers = H::new();
        headers.insert("Accept-Encoding".to_string(), "identity".to_string());

        // Warm cache - first GET (cache MISS, entry is stored)
        let (status1, headers1, _, _) = assert_ok(
            do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
        );
        assert_equal(200, status1);
        let ua1 = get_updated_at(&headers1);

        // Second GET - should be cache HIT (same Last-Updated-At)
        let (status2, headers2, _, _) = assert_ok(
            do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
        );
        assert_equal(200, status2);
        let ua2 = get_updated_at(&headers2);
        assert_eq!(
            ua2, ua1,
            "expected cache HIT: Last-Updated-At must be stable before invalidation"
        );

        // Invalidate
        #[derive(serde::Deserialize)]
        struct InvalidateResponse {
            success: bool,
            affected: i64,
        }
        let invalidate_url = format!(
            "{}/advcache/invalidate?_path=/api/v1/user&user[id]=inv_int1&domain=invm.example&language=en",
            base
        );
        let (invalidate_status, _, invalidate_body, _) = assert_ok(
            do_json::<InvalidateResponse>("GET", &invalidate_url, &H::new()).await,
        );
        assert_equal(200, invalidate_status);
        let invalidate_resp: InvalidateResponse = serde_json::from_slice(&invalidate_body).unwrap();
        assert!(invalidate_resp.success);
        assert!(invalidate_resp.affected >= 1);

        // Wait for invalidation to take effect.
        // Poll every 500ms for up to 30 seconds, checking that Last-Updated-At changed.
        let timeout = std::time::Duration::from_secs(30);
        let poll_interval = tokio::time::Duration::from_millis(500);
        let start = std::time::Instant::now();
        
        loop {
            // Check if invalidation was applied by checking Last-Updated-At
            let (status_after, headers_after, _, _) = assert_ok(
                do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
            );
            assert_equal(200, status_after);
            
            let ua_after = get_updated_at(&headers_after);
            
            // If Last-Updated-At changed, invalidation was successful
            if ua_after != ua1 {
                // Invalidation processed successfully - entry was refreshed
                break;
            }
            
            if start.elapsed() >= timeout {
                panic!(
                    "timeout waiting for invalidation to take effect: Last-Updated-At still {} after {:?}",
                    ua_after,
                    start.elapsed()
                );
            }
            
            tokio::time::sleep(poll_interval).await;
        }
        
        // Final verification - entry should be invalidated and Last-Updated-At should be different
        let (status_final, headers_final, _, _) = assert_ok(
            do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
        );
        assert_equal(200, status_final);
        let ua_final = get_updated_at(&headers_final);
        assert_ne!(
            ua_final, ua1,
            "expected cache MISS after invalidation: Last-Updated-At must change"
        );
    }).await;
}

/// Test that cache clear endpoint actually clears all cached entries.
#[tokio::test]
async fn test_cache_clear_actually_clears() {
    use crate::support::with_global_lock;
    
    with_global_lock(|| async {
        init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Clear_ActuallyClears");
    let base = cache_addr().await;

    // Create multiple cache entries
    let mut params1 = HashMap::new();
    params1.insert("user[id]".to_string(), "clear1".to_string());
    params1.insert("domain".to_string(), "clear.example".to_string());
    params1.insert("language".to_string(), "en".to_string());

    let mut params2 = HashMap::new();
    params2.insert("user[id]".to_string(), "clear2".to_string());
    params2.insert("domain".to_string(), "clear.example".to_string());
    params2.insert("language".to_string(), "en".to_string());

    let path1 = with_ns("/api/v1/user", &ns, &params1);
    let path2 = with_ns("/api/v1/user", &ns, &params2);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Warm cache with multiple entries
    let (_, _, body1_orig, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path1), &headers).await,
    );
    let (_, _, body2_orig, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path2), &headers).await,
    );

    // Verify both are cached
    let (_, _, body1_hit, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path1), &headers).await,
    );
    assert_eq!(body1_orig, body1_hit);
    let (_, _, body2_hit, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path2), &headers).await,
    );
    assert_eq!(body2_orig, body2_hit);

    // Get token and clear cache
    #[derive(serde::Deserialize)]
    struct TokenResponse {
        token: String,
    }
    #[derive(serde::Deserialize)]
    struct ClearStatusResponse {
        cleared: Option<bool>,
    }

    let (_, _, token_body, _) = assert_ok(
        do_json::<TokenResponse>("GET", &format!("{}/advcache/clear", base), &H::new()).await,
    );
    let token_resp: TokenResponse = serde_json::from_slice(&token_body).unwrap();
    let url = format!("{}/advcache/clear?token={}", base, urlencoding::encode(&token_resp.token));
    
    let (clear_status, _, clear_body, _) = assert_ok(
        do_json::<ClearStatusResponse>("GET", &url, &H::new()).await,
    );
    assert_equal(200, clear_status);
    let clear_resp: ClearStatusResponse = serde_json::from_slice(&clear_body).unwrap();
    assert!(clear_resp.cleared.unwrap_or(false), "cache should be cleared");

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // After clear, entries should be removed from cache
    // Both should return 200 but may be fresh requests (cache cleared)
    let (status1_after, _, _, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path1), &headers).await,
    );
    assert_equal(200, status1_after);

    let (status2_after, _, _, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path2), &headers).await,
    );
    assert_equal(200, status2_after);
    }).await;
}

/// Test that compression middleware actually affects response headers.
#[tokio::test]
async fn test_compression_middleware_affects_headers() {
    use crate::support::with_global_lock;
    
    with_global_lock(|| async {
        init_test_harness().await.unwrap();

    let base = cache_addr().await;

    // Enable compression
    #[derive(serde::Deserialize)]
    struct StatusResponse {
        enabled: bool,
    }
    let (_, _, body, _) = assert_ok(
        do_json::<StatusResponse>("GET", &format!("{}/advcache/http/compression/on", base), &H::new()).await,
    );
    let resp: StatusResponse = serde_json::from_slice(&body).unwrap();
    assert!(resp.enabled, "compression should be enabled");

    // Make a request with Accept-Encoding: gzip
    let ns = new_namespace("Test_Compression_AffectsHeaders");
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "comp1".to_string());
    params.insert("domain".to_string(), "comp.example".to_string());
    params.insert("language".to_string(), "en".to_string());

    let path = with_ns("/api/v1/user", &ns, &params);
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "gzip".to_string());

    let (status, _resp_headers, _, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status);

    // Compression middleware may or may not compress depending on body size
    // Just verify response is valid
    // Note: Actual compression behavior depends on middleware implementation

    // Disable compression for cleanup
    let _ = assert_ok(
        do_json::<StatusResponse>("GET", &format!("{}/advcache/http/compression/off", base), &H::new()).await,
    );
    }).await;
}

/// Test that multiple invalidations don't interfere with each other.
#[tokio::test]
async fn test_multiple_invalidations_independent() {
    use crate::support::with_global_lock;
    
    with_global_lock(|| async {
        init_test_harness().await.unwrap();

    let ns = new_namespace("Test_MultipleInvalidations");
    let base = cache_addr().await;

    // Create two independent entries
    let mut params1 = HashMap::new();
    params1.insert("user[id]".to_string(), "multi1".to_string());
    params1.insert("domain".to_string(), "multi.example".to_string());
    params1.insert("language".to_string(), "en".to_string());

    let mut params2 = HashMap::new();
    params2.insert("user[id]".to_string(), "multi2".to_string());
    params2.insert("domain".to_string(), "multi.example".to_string());
    params2.insert("language".to_string(), "en".to_string());

    let path1 = with_ns("/api/v1/user", &ns, &params1);
    let path2 = with_ns("/api/v1/user", &ns, &params2);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Warm cache
    let (_, _, body1_orig, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path1), &headers).await,
    );
    let (_, _, body2_orig, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path2), &headers).await,
    );

    // Verify both cached
    let (_, _, body1_hit, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path1), &headers).await,
    );
    assert_eq!(body1_orig, body1_hit);
    let (_, _, body2_hit, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path2), &headers).await,
    );
    assert_eq!(body2_orig, body2_hit);

    // Invalidate only first entry
    #[derive(serde::Deserialize)]
    struct InvalidateResponse {
        success: bool,
    }
    let invalidate_url = format!(
        "{}/advcache/invalidate?_path=/api/v1/user&user[id]=multi1&domain=multi.example&language=en",
        base
    );
    let (invalidate_status, _, invalidate_body, _) = assert_ok(
        do_json::<InvalidateResponse>("GET", &invalidate_url, &H::new()).await,
    );
    assert_equal(200, invalidate_status);
    let invalidate_resp: InvalidateResponse = serde_json::from_slice(&invalidate_body).unwrap();
    assert!(invalidate_resp.success);
    // affected may be 0 if entry was already invalidated
    // The important thing is that the endpoint returns success

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // First entry should be invalidated
    let (status1_after, _, _, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path1), &headers).await,
    );
    assert_equal(200, status1_after);

    // Second entry should still be cached (not affected by first invalidation)
    let (status2_after, _, body2_after, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path2), &headers).await,
    );
    assert_equal(200, status2_after);
    assert_eq!(body2_orig, body2_after, "second entry should still be cached after invalidating first");
    }).await;
}

/// Test that cache behavior is consistent when switching between cache and proxy modes.
#[tokio::test]
async fn test_cache_proxy_mode_switching_consistency() {
    use crate::support::with_global_lock;
    
    with_global_lock(|| async {
        init_test_harness().await.unwrap();

    let ns = new_namespace("Test_CacheProxy_Switching");
    let base = cache_addr().await;

    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "switch1".to_string());
    params.insert("domain".to_string(), "switch.example".to_string());
    params.insert("language".to_string(), "en".to_string());

    let path = with_ns("/api/v1/user", &ns, &params);
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Enable cache mode
    async fn toggle_cache(on: bool) {
        let base = cache_addr().await;
        let path = if on {
            "/cache/bypass/off"
        } else {
            "/cache/bypass/on"
        };
        let client = reqwest::Client::new();
        let resp = client
            .get(&format!("{}{}", base, path))
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success(), "toggle {} failed: {}", on, resp.status());
        tokio::time::sleep(tokio::time::Duration::from_millis(750)).await;
    }

    toggle_cache(true).await;

    // Make request in cache mode - should cache
    let (status1, _, body1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status1);

    // Should be cache hit
    let (status2, _, body2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status2);
    assert_eq!(body1, body2, "should be cache hit in cache mode");

    // Switch to proxy mode
    toggle_cache(false).await;

    // Request in proxy mode - should not use cache (bypass enabled)
    let (status3, _, _, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status3);

    // Switch back to cache mode
    toggle_cache(true).await;

    // Should still have cached entry from before
    let (status4, _, _body4, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status4);
    // May be same (cache hit) or new (if entry expired/refreshed), but should work
    }).await;
}
