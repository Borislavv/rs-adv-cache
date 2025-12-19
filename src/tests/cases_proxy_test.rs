// Integration tests for proxy functionality.

use std::collections::HashMap;
use crate::support::{
    assert_equal, assert_ok, cache_addr, do_json, init_test_harness, new_namespace, with_ns, H,
};
use once_cell::sync::Lazy;
use tokio::sync::Mutex as AsyncMutex;

static CACHE_MODE_LOCK: Lazy<AsyncMutex<()>> = Lazy::new(|| AsyncMutex::new(()));

#[tokio::test]
async fn test_toggle_cache_on_off_roundtrip() {
    init_test_harness().await.unwrap();

    let _guard = CACHE_MODE_LOCK.lock().await;

    let ns = new_namespace("Test_Toggle_CacheOnOff_Roundtrip");
    let base = cache_addr().await;

    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "9903".to_string());
    params.insert("domain".to_string(), "toggle.example".to_string());
    params.insert("language".to_string(), "en".to_string());

    let u = with_ns("/api/v1/user", &ns, &params);

    // 1) cache mode
    toggle_cache(true).await;
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, _, _) =
        assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await); // warm
    assert_equal(200, st);
    let (st, hdr, _, _) =
        assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await); // hit
    assert_equal(200, st);

    let hbh = vec![
        "Connection",
        "Proxy-Connection",
        "Keep-Alive",
        "Proxy-Authenticate",
        "Proxy-Authorization",
        "TE",
        "Trailer",
        "Transfer-Encoding",
        "Upgrade",
    ];
    for h in &hbh {
        if hdr.contains_key(&h.to_lowercase()) {
            panic!(
                "cache mode (hit): hop-by-hop header leaked: {}={:?}",
                h,
                hdr.get(&h.to_lowercase())
            );
        }
    }
    if !hdr.contains_key("content-type") {
        panic!("cache mode (hit): expected Content-Type header");
    }

    // 2) proxy mode
    toggle_cache(false).await;
    let (st, hdr, _, _) =
        assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await);
    assert_equal(200, st);
    for h in &hbh {
        if hdr.contains_key(&h.to_lowercase()) {
            panic!(
                "proxy mode: hop-by-hop header leaked: {}={:?}",
                h,
                hdr.get(&h.to_lowercase())
            );
        }
    }
    if !hdr.contains_key("content-type") {
        panic!("proxy mode: expected Content-Type header");
    }

    // 3) restore
    toggle_cache(true).await;
}

#[tokio::test]
async fn test_proxy_mode_whitelist_noise_ignored_semantics() {
    init_test_harness().await.unwrap();

    let _guard = CACHE_MODE_LOCK.lock().await;
    toggle_cache(false).await;

    let ns = new_namespace("test_proxy_mode_whitelist_noise_ignored_semantics");
    let base = cache_addr().await;
    let path = "/api/v1/client";

    use url::form_urlencoded;
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("user[id]", "9902");
    serializer.append_pair("domain", "proxy.example");
    serializer.append_pair("language", "en");
    serializer.append_pair("ns", &ns);
    let u_base = format!("{}?{}", path, serializer.finish());
    let u_noise = format!("{}&utm_source=x&foo=bar&ref=y", u_base);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_base), &headers).await,
    );
    assert_equal(200, st);

    headers.insert("X-Debug".to_string(), "1".to_string());
    let (st, _, b2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_noise), &headers).await,
    );
    assert_equal(200, st);

    use sha1::{Digest, Sha1};
    let mut hasher1 = Sha1::new();
    hasher1.update(&b1);
    let h1 = hex::encode(hasher1.finalize());
    let mut hasher2 = Sha1::new();
    hasher2.update(&b2);
    let h2 = hex::encode(hasher2.finalize());

    if h1 != h2 {
        panic!("proxy mode: noise in query/headers must not affect body");
    }

    toggle_cache(true).await;
}

#[tokio::test]
async fn test_proxy_forwarded_host_passed_to_upstream() {
    init_test_harness().await.unwrap();

    let _guard = CACHE_MODE_LOCK.lock().await;
    toggle_cache(false).await; // proxy mode

    let ns = new_namespace("test_proxy_forwarded_host_passed_to_upstream");
    let base = cache_addr().await;

    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "9904".to_string());
    params.insert("domain".to_string(), "forwarded.example".to_string());
    params.insert("language".to_string(), "en".to_string());

    let u = with_ns("/api/v1/user", &ns, &params);

    // Test 1: X-Forwarded-Host should be used as Host header
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    headers.insert("X-Forwarded-Host".to_string(), "forwarded.example.com:8080".to_string());
    
    let (st, _, body, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await
    );
    assert_equal(200, st);
    
    let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    if let Some(echo) = response.get("echo") {
        if let Some(host) = echo.get("host") {
            assert_eq!(
                host.as_str().unwrap(),
                "forwarded.example.com:8080",
                "X-Forwarded-Host should be passed as Host header to upstream"
            );
        } else {
            panic!("echo.host not found in response");
        }
    } else {
        panic!("echo not found in response");
    }

    // Test 2: Host header should be used as fallback when X-Forwarded-Host is absent
    // Use a different namespace to avoid cache interference
    let ns2 = new_namespace("test_proxy_forwarded_host_fallback");
    let mut params2 = HashMap::new();
    params2.insert("user[id]".to_string(), "9905".to_string());
    params2.insert("domain".to_string(), "forwarded.example".to_string());
    params2.insert("language".to_string(), "en".to_string());
    let u2 = with_ns("/api/v1/user", &ns2, &params2);
    
    let mut headers2 = H::new();
    headers2.insert("Accept-Encoding".to_string(), "identity".to_string());
    headers2.insert("Host".to_string(), "fallback.example.com:9090".to_string());
    
    let (st2, _, body2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, u2), &headers2).await
    );
    assert_equal(200, st2);
    
    let response2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    if let Some(echo) = response2.get("echo") {
        if let Some(host) = echo.get("host") {
            assert_eq!(
                host.as_str().unwrap(),
                "fallback.example.com:9090",
                "Host header should be passed as Host header to upstream when X-Forwarded-Host absent"
            );
        } else {
            panic!("echo.host not found in response");
        }
    } else {
        panic!("echo not found in response");
    }

    // Test 3: X-Forwarded-Host takes precedence over Host
    // Use a different namespace to avoid cache interference
    let ns3 = new_namespace("test_proxy_forwarded_host_precedence");
    let mut params3 = HashMap::new();
    params3.insert("user[id]".to_string(), "9906".to_string());
    params3.insert("domain".to_string(), "forwarded.example".to_string());
    params3.insert("language".to_string(), "en".to_string());
    let u3 = with_ns("/api/v1/user", &ns3, &params3);
    
    let mut headers3 = H::new();
    headers3.insert("Accept-Encoding".to_string(), "identity".to_string());
    headers3.insert("X-Forwarded-Host".to_string(), "preferred.example.com:7070".to_string());
    headers3.insert("Host".to_string(), "ignored.example.com:6060".to_string());
    
    let (st3, _, body3, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, u3), &headers3).await
    );
    assert_equal(200, st3);
    
    let response3: serde_json::Value = serde_json::from_slice(&body3).unwrap();
    if let Some(echo) = response3.get("echo") {
        if let Some(host) = echo.get("host") {
            assert_eq!(
                host.as_str().unwrap(),
                "preferred.example.com:7070",
                "X-Forwarded-Host should take precedence over Host header"
            );
        } else {
            panic!("echo.host not found in response");
        }
    } else {
        panic!("echo not found in response");
    }

    toggle_cache(true).await;
}

#[tokio::test]
async fn test_hop_by_hop_headers_not_sent_to_upstream() {
    init_test_harness().await.unwrap();

    let _guard = CACHE_MODE_LOCK.lock().await;
    toggle_cache(false).await; // proxy mode

    let ns = new_namespace("test_hop_by_hop_headers_not_sent_to_upstream");
    let base = cache_addr().await;

    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "9905".to_string());
    params.insert("domain".to_string(), "hopbyhop.example".to_string());
    params.insert("language".to_string(), "en".to_string());

    let u = with_ns("/api/v1/user", &ns, &params);

    // Send request with hop-by-hop headers
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    headers.insert("Connection".to_string(), "keep-alive".to_string());
    headers.insert("Proxy-Connection".to_string(), "keep-alive".to_string());
    headers.insert("Keep-Alive".to_string(), "timeout=5".to_string());
    headers.insert("Transfer-Encoding".to_string(), "chunked".to_string());
    headers.insert("Upgrade".to_string(), "websocket".to_string());
    
    let (st, _, body, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await
    );
    assert_equal(200, st);
    
    let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    if let Some(echo) = response.get("echo") {
        if let Some(hop_by_hop_received) = echo.get("hop_by_hop_received") {
            if let Some(arr) = hop_by_hop_received.as_array() {
                if !arr.is_empty() {
                    panic!(
                        "hop-by-hop headers were sent to upstream: {:?}",
                        arr
                    );
                }
            }
        }
    }

    toggle_cache(true).await;
}

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
    if !resp.status().is_success() {
        panic!("toggle {} failed: {}", on, resp.status());
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
}
