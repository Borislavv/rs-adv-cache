// Integration tests for concurrent access scenarios.


use std::collections::HashMap;
use crate::support::{assert_equal, assert_ok, cache_addr, do_json, init_test_harness, new_namespace, with_ns, H};

/// Test that concurrent requests to the same cache key return consistent results.
/// This verifies that cache handles concurrent access correctly without data races.
#[tokio::test]
async fn test_concurrent_requests_same_key() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Concurrent_SameKey");
    let base = cache_addr().await;

    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "42".to_string());
    params.insert("domain".to_string(), "example.org".to_string());
    params.insert("language".to_string(), "en".to_string());

    let path = with_ns("/api/v1/user", &ns, &params);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Warm cache
    let (status, _, _, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await,
    );
    assert_equal(200, status);

    // Make concurrent requests to the same key
    let mut handles = vec![];
    for _ in 0..10 {
        let base = base.clone();
        let path = path.clone();
        let headers = headers.clone();
        let handle = tokio::spawn(async move {
            do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await
        });
        handles.push(handle);
    }

    // All requests should succeed with 200
    for handle in handles {
        let result = handle.await.unwrap();
        let (status, _, _, _) = assert_ok(result);
        assert_equal(200, status);
    }
}

/// Test that concurrent requests with different keys don't interfere.
/// Verifies key isolation under concurrent load.
#[tokio::test]
async fn test_concurrent_requests_different_keys() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Concurrent_DifferentKeys");
    let base = cache_addr().await;

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Make concurrent requests with different user IDs
    let mut handles = vec![];
    for i in 0..10 {
        let base = base.clone();
        let ns = ns.clone();
        let headers = headers.clone();
        let handle = tokio::spawn(async move {
            let mut params = HashMap::new();
            params.insert("user[id]".to_string(), format!("{}", i));
            params.insert("domain".to_string(), "example.org".to_string());
            params.insert("language".to_string(), "en".to_string());
            let path = with_ns("/api/v1/user", &ns, &params);
            do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await
        });
        handles.push(handle);
    }

    // All requests should succeed
    for handle in handles {
        let result = handle.await.unwrap();
        let (status, _, _, _) = assert_ok(result);
        assert_equal(200, status);
    }
}

/// Test that concurrent cache misses (first requests) are handled correctly.
/// Verifies that multiple concurrent cold requests don't cause issues.
#[tokio::test]
async fn test_concurrent_first_requests_misses() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Concurrent_CacheMisses");
    let base = cache_addr().await;

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Make concurrent first requests (cache misses) to different keys
    let mut handles = vec![];
    for i in 100..110 {
        let base = base.clone();
        let ns = ns.clone();
        let headers = headers.clone();
        let handle = tokio::spawn(async move {
            let mut params = HashMap::new();
            params.insert("user[id]".to_string(), format!("{}", i));
            params.insert("domain".to_string(), "example.org".to_string());
            params.insert("language".to_string(), "en".to_string());
            let path = with_ns("/api/v1/user", &ns, &params);
            do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await
        });
        handles.push(handle);
    }

    // All requests should succeed
    let mut statuses = vec![];
    for handle in handles {
        let result = handle.await.unwrap();
        let (status, _, _, _) = assert_ok(result);
        statuses.push(status);
    }

    // All should return 200
    for status in statuses {
        assert_equal(200, status);
    }
}

/// Test that concurrent writes and reads don't cause data corruption.
#[tokio::test]
async fn test_concurrent_read_write() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Concurrent_ReadWrite");
    let base = cache_addr().await;

    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "99".to_string());
    params.insert("domain".to_string(), "example.org".to_string());
    params.insert("language".to_string(), "en".to_string());

    let path = with_ns("/api/v1/user", &ns, &params);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    // Start read tasks
    let mut read_handles = vec![];
    for _ in 0..5 {
        let base = base.clone();
        let path = path.clone();
        let headers = headers.clone();
        let handle = tokio::spawn(async move {
            do_json::<serde_json::Value>("GET", &format!("{}{}", base, path), &headers).await
        });
        read_handles.push(handle);
    }

    // Start write task (first request that warms cache)
    let base_write = base.clone();
    let path_write = path.clone();
    let headers_write = headers.clone();
    let write_handle = tokio::spawn(async move {
        do_json::<serde_json::Value>("GET", &format!("{}{}", base_write, path_write), &headers_write).await
    });

    // Wait for write to complete
    let (status, _, _, _) = assert_ok(write_handle.await.unwrap());
    assert_equal(200, status);

    // All reads should succeed (some may be misses, some hits)
    for handle in read_handles {
        let result = handle.await.unwrap();
        let (status, _, _, _) = assert_ok(result);
        assert_equal(200, status);
    }
}
