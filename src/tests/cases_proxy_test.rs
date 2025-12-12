// Integration tests for proxy functionality.

#[path = "support/mod.rs"]
mod support;

use std::collections::HashMap;
use support::{assert_equal, assert_ok, do_json, new_namespace, with_ns, H, cache_addr, init_test_harness};

#[tokio::test]
async fn test_toggle_cache_on_off_roundtrip() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_toggle_cache_on_off_roundtrip");
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
    let (st, _, _, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await); // warm
    assert_equal(200, st);
    let (st, hdr, _, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await); // hit
    assert_equal(200, st);
    
    let hbh = vec!["Connection", "Proxy-Connection", "Keep-Alive", "Proxy-Authenticate", "Proxy-Authorization", "TE", "Trailer", "Transfer-Encoding", "Upgrade"];
    for h in &hbh {
        if hdr.contains_key(&h.to_lowercase()) {
            panic!("cache mode (hit): hop-by-hop header leaked: {}={:?}", h, hdr.get(&h.to_lowercase()));
        }
    }
    if !hdr.contains_key("content-type") {
        panic!("cache mode (hit): expected Content-Type header");
    }
    
    // 2) proxy mode
    toggle_cache(false).await;
    let (st, hdr, _, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await);
    assert_equal(200, st);
    for h in &hbh {
        if hdr.contains_key(&h.to_lowercase()) {
            panic!("proxy mode: hop-by-hop header leaked: {}={:?}", h, hdr.get(&h.to_lowercase()));
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
    let (st, _, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_base), &headers).await);
    assert_equal(200, st);
    
    headers.insert("X-Debug".to_string(), "1".to_string());
    let (st, _, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_noise), &headers).await);
    assert_equal(200, st);
    
    use sha1::{Sha1, Digest};
    use hex;
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

async fn toggle_cache(on: bool) {
    let base = cache_addr().await;
    let path = if on { "/advcache/bypass/off" } else { "/advcache/bypass/on" };
    let client = reqwest::Client::new();
    let resp = client.get(&format!("{}{}", base, path)).send().await.unwrap();
    if !resp.status().is_success() {
        panic!("toggle {} failed: {}", on, resp.status());
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
}

