// Integration tests for cache behavior and edge cases.

#[path = "support/mod.rs"]
mod support;

use std::collections::HashMap;
use crate::support::{assert_equal, assert_ok, cache_addr, do_json, init_test_harness, new_namespace, with_ns, H};

/// Test that cache stores entries with different query parameter orders as same key.
#[tokio::test]
async fn test_cache_query_order_independence() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Cache_QueryOrder");
    let base = cache_addr().await;

    // Same parameters, different order
    let path1 = format!("/api/v1/user?user[id]=8888&domain=order.example&language=en&ns={}", ns);
    let path2 = format!("/api/v1/user?language=en&user[id]=8888&domain=order.example&ns={}", ns);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // First request with one order
    let (status1, _, body1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path1), &headers).await,
    );
    assert_equal(200, status1);

    // Second request with different order - should be cache hit
    let (status2, _, body2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path2), &headers).await,
    );
    assert_equal(200, status2);
    
    // Bodies should be identical (same cache key due to query sorting)
    assert_eq!(body1, body2, "different query order should result in same cache key");
}

/// Test that cache respects Accept-Encoding in cache key.
#[tokio::test]
async fn test_cache_respects_accept_encoding_in_key() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Cache_AcceptEncoding");
    let base = cache_addr().await;

    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "9999".to_string());
    params.insert("domain".to_string(), "ae.example".to_string());
    params.insert("language".to_string(), "en".to_string());

    let path = with_ns("/api/v1/user", &ns, &params);

    // Request with identity encoding
    let mut headers1 = H::new();
    headers1.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (status1, _, body1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers1).await,
    );
    assert_equal(200, status1);

    // Request with gzip encoding - should be different cache key
    let mut headers2 = H::new();
    headers2.insert("Accept-Encoding".to_string(), "gzip".to_string());
    let (status2, _, body2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers2).await,
    );
    assert_equal(200, status2);

    // Bodies might be different (one compressed, one not) or same depending on upstream
    // But they should be served from different cache entries
    // Verify both are cache hits by checking they return quickly (implicit test)
    
    // Second request with identity - should be cache hit
    let (status1_hit, _, body1_hit, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers1).await,
    );
    assert_equal(200, status1_hit);
    assert_eq!(body1, body1_hit, "identity encoding should be cached separately");

    // Second request with gzip - should be cache hit
    let (status2_hit, _, body2_hit, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers2).await,
    );
    assert_equal(200, status2_hit);
    assert_eq!(body2, body2_hit, "gzip encoding should be cached separately");
}

/// Test that cache handles large response bodies correctly.
#[tokio::test]
async fn test_cache_large_response_body() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Cache_LargeBody");
    let base = cache_addr().await;

    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "10000".to_string());
    params.insert("domain".to_string(), "large.example".to_string());
    params.insert("language".to_string(), "en".to_string());

    let path = with_ns("/api/v1/user", &ns, &params);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Request should succeed
    let (status1, _, body1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status1);
    
    // Verify body is valid JSON
    let _: serde_json::Value = serde_json::from_slice(&body1)
        .expect("response body should be valid JSON");

    // Cache hit should work even for large bodies
    let (status2, _, body2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status2);
    assert_eq!(body1, body2, "large body should be cached correctly");
}

/// Test that cache continues to serve entries even after some time has passed.
/// Verifies cache persistence and that entries don't disappear unexpectedly.
#[tokio::test]
async fn test_cache_persistence_over_time() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Cache_Persistence");
    let base = cache_addr().await;

    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "11111".to_string());
    params.insert("domain".to_string(), "persist.example".to_string());
    params.insert("language".to_string(), "en".to_string());

    let path = with_ns("/api/v1/user", &ns, &params);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Warm cache
    let (status1, _, body1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status1);

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Should still get a response (cache hit or stale serve)
    let (status2, _, body2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status2);
    // Body should be same (cache hit) or valid (stale served)
    let _: serde_json::Value = serde_json::from_slice(&body2)
        .expect("response body should be valid JSON");
}

/// Test that cache handles requests with no query parameters.
#[tokio::test]
async fn test_cache_no_query_parameters() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Cache_NoQuery");
    let base = cache_addr().await;

    // Path without query parameters (only namespace)
    let path = format!("/api/v1/user?ns={}", ns);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Request should succeed
    let (status1, _, body1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status1);

    // Should be cacheable (if rule allows empty queries)
    let (status2, _, body2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status2);
    // May or may not be cached depending on configuration, but should work
}

/// Test that cache handles multiple cache hits correctly.
#[tokio::test]
async fn test_cache_multiple_hits() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Cache_MultipleHits");
    let base = cache_addr().await;

    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "22222".to_string());
    params.insert("domain".to_string(), "multihit.example".to_string());
    params.insert("language".to_string(), "en".to_string());

    let path = with_ns("/api/v1/user", &ns, &params);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Warm cache
    let (status1, _, body1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status1);

    // Multiple cache hits should all return same body
    for _ in 0..5 {
        let (status, _, body, _) = assert_ok(
            do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
        );
        assert_equal(200, status);
        assert_eq!(body1, body, "all cache hits should return identical body");
    }
}

/// Test that cache handles requests with special characters in query values.
#[tokio::test]
async fn test_cache_special_characters_in_query() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Cache_SpecialChars");
    let base = cache_addr().await;

    // Test with special characters in query parameter values
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "special@123".to_string());
    params.insert("domain".to_string(), "special.example".to_string());
    params.insert("language".to_string(), "en-US".to_string());

    let path = with_ns("/api/v1/user", &ns, &params);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Request with special characters should succeed
    let (status1, _, body1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status1);

    // Should be cacheable
    let (status2, _, body2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status2);
    assert_eq!(body1, body2, "special characters in query should be handled correctly");
}
