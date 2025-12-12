// Integration tests for whitelist behavior.

#[path = "support/mod.rs"]
mod support;

use std::collections::HashMap;
use support::{assert_equal, assert_ok, do_json, new_namespace, with_ns, H, cache_addr, init_test_harness, phash};

#[tokio::test]
async fn test_whitelist_query_ignores_extras() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_whitelist_query_ignores_extras");
    let base = cache_addr().await;
    let path = "/api/v1/user";
    
    use url::form_urlencoded;
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("user[id]", "5001");
    serializer.append_pair("domain", "wl.example");
    serializer.append_pair("language", "en");
    serializer.append_pair("ns", &ns);
    let u_base = format!("{}?{}", path, serializer.finish());
    let u_noise = format!("{}&utm_source=x&foo=bar&ref=y", u_base);
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_base), &headers).await);
    assert_equal(200, st);
    
    let (st, _, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_noise), &headers).await);
    assert_equal(200, st);
    
    if phash(&b1) != phash(&b2) {
        panic!("non-whitelisted query params must not affect response body");
    }
}

#[tokio::test]
async fn test_whitelist_headers_ignores_extras() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_whitelist_headers_ignores_extras");
    let base = cache_addr().await;
    
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "5002".to_string());
    params.insert("domain".to_string(), "wl.example".to_string());
    params.insert("language".to_string(), "en".to_string());
    
    let u = with_ns("/api/v1/user", &ns, &params);
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await);
    assert_equal(200, st);
    
    headers.insert("X-Debug".to_string(), "1".to_string());
    headers.insert("Accept-Language".to_string(), "ru".to_string());
    let (st, _, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await);
    assert_equal(200, st);
    
    if phash(&b1) != phash(&b2) {
        panic!("non-whitelisted request headers must not affect response body");
    }
}

#[tokio::test]
async fn test_response_headers_filtered_on_cache_hit() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_response_headers_filtered_on_cache_hit");
    let base = cache_addr().await;
    
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "9003".to_string());
    params.insert("domain".to_string(), "wl.example".to_string());
    params.insert("language".to_string(), "en".to_string());
    
    let u = with_ns("/api/v1/user", &ns, &params);
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    
    // Warm the cache
    let (status, _, _, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await);
    assert_equal(200, status);
    
    // Cache hit: inspect response headers
    let (status, hdr, _, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await);
    assert_equal(200, status);
    
    // 1) Hop-by-hop headers must be stripped on cache hits
    let hbh = vec!["Connection", "Proxy-Connection", "Keep-Alive", "Proxy-Authenticate", "Proxy-Authorization", "TE", "Trailer", "Transfer-Encoding", "Upgrade"];
    for h in &hbh {
        if hdr.contains_key(&h.to_lowercase()) {
            panic!("hop-by-hop header leaked on cache hit: {}={:?}", h, hdr.get(&h.to_lowercase()));
        }
    }
    
    // 2) Content-Type should be present
    if !hdr.contains_key("content-type") {
        panic!("cache hit: expected Content-Type");
    }
}

