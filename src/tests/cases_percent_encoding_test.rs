// Integration tests for percent encoding equivalence.

#[path = "support/mod.rs"]
mod support;

use std::collections::HashMap;
use support::{assert_equal, assert_ok, do_json, new_namespace, H, cache_addr, init_test_harness, phash};

#[tokio::test]
async fn test_query_percent_encoding_space_plus_vs_20_equivalent() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_query_percent_encoding_space_plus_vs_20_equivalent");
    let base = cache_addr().await;
    let path = "/api/v1/user";
    
    use urlencoding;
    let q_plus = format!("user%5Bid%5D=123&domain=pe.example&language=en&picked=helloworld+foobarbazz&ns={}", urlencoding::encode(&ns));
    let q_pct20 = format!("user%5Bid%5D=123&domain=pe.example&language=en&picked=helloworld%20foobarbazz&ns={}", urlencoding::encode(&ns));
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}?{}", base, path, q_plus), &headers).await);
    assert_equal(200, st);
    let (st, _, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}?{}", base, path, q_pct20), &headers).await);
    assert_equal(200, st);
    if phash(&b1) != phash(&b2) {
        panic!("+ vs %20 must be equivalent for spaces");
    }
}

#[tokio::test]
async fn test_query_percent_encoding_literal_plus_different_from_space() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_query_percent_encoding_literal_plus_different_from_space");
    let base = cache_addr().await;
    let path = "/api/v1/user";
    
    use urlencoding;
    let q_space = format!("user%5Bid%5D=123&domain=pe.example&language=en&picked=helloworld+foobarbazz&ns={}", urlencoding::encode(&ns)); // space
    let q_plus = format!("user%5Bid%5D=123&domain=pe.example&language=en&picked=helloworld%2Bfoobarbazz&ns={}", urlencoding::encode(&ns)); // '+' literal
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}?{}", base, path, q_space), &headers).await);
    assert_equal(200, st);
    let (st, _, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}?{}", base, path, q_plus), &headers).await);
    assert_equal(200, st);
    if phash(&b1) == phash(&b2) {
        panic!("literal '+' (%2B) must not equal a space encoding");
    }
}

#[tokio::test]
async fn test_query_percent_encoding_hex_case_insensitive_equivalent() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_query_percent_encoding_hex_case_insensitive_equivalent");
    let base = cache_addr().await;
    let path = "/api/v1/client";
    
    use urlencoding;
    let q_lower = format!("user%5Bid%5D=286&domain=pe.example&language=en&picked=a%2fb&ns={}", urlencoding::encode(&ns));
    let q_upper = format!("user%5Bid%5D=286&domain=pe.example&language=en&picked=a%2FB&ns={}", urlencoding::encode(&ns));
    
    #[derive(serde::Deserialize)]
    struct RespPayload {
        title: String,
        description: String,
        #[serde(rename = "echo")]
        echo: EchoPayload,
    }
    
    #[derive(serde::Deserialize)]
    struct EchoPayload {
        path: String,
        query: String,
        ae: String,
    }
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b1, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}?{}", base, path, q_lower), &headers).await);
    assert_equal(200, st);
    let (st, _, b2, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}?{}", base, path, q_upper), &headers).await);
    assert_equal(200, st);
    
    let p1: RespPayload = serde_json::from_slice(&b1).unwrap();
    let p2: RespPayload = serde_json::from_slice(&b2).unwrap();
    
    if p1.title != p2.title || p1.description != p2.description || p1.echo.path != p2.echo.path || p1.echo.ae != p2.echo.ae {
        panic!("semantic fields must match for %2f vs %2F");
    }
}

#[tokio::test]
async fn test_query_percent_encoding_utf8_equivalent() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_query_percent_encoding_utf8_equivalent");
    let base = cache_addr().await;
    let path = "/api/v1/user";
    
    let raw = "ставка";
    use url::form_urlencoded;
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("user[id]", "287");
    serializer.append_pair("domain", "pe.example");
    serializer.append_pair("language", "ru");
    serializer.append_pair("picked", raw);
    serializer.append_pair("ns", &ns);
    let q_raw = serializer.finish();
    
    use urlencoding;
    let q_enc = format!("user%5Bid%5D=287&domain=pe.example&language=ru&picked={}&ns={}", urlencoding::encode(raw), urlencoding::encode(&ns));
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}?{}", base, path, q_raw), &headers).await);
    assert_equal(200, st);
    let (st, _, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}?{}", base, path, q_enc), &headers).await);
    assert_equal(200, st);
    if phash(&b1) != phash(&b2) {
        panic!("UTF-8 raw vs percent-encoded must be equivalent");
    }
}

#[tokio::test]
async fn test_percent_encoding_double_encoding_not_equivalent() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_percent_encoding_double_encoding_not_equivalent");
    let base = cache_addr().await;
    let path = "/api/v1/user";
    
    use urlencoding;
    let q_double = format!("user%5Bid%5D=303&domain=pe2.example&language=en&picked=a%252Fb&ns={}", urlencoding::encode(&ns)); // -> "a%2Fb"
    let q_single = format!("user%5Bid%5D=303&domain=pe2.example&language=en&picked=a%2Fb&ns={}", urlencoding::encode(&ns)); // -> "a/b"
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}?{}", base, path, q_double), &headers).await);
    assert_equal(200, st);
    let (st, _, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}?{}", base, path, q_single), &headers).await);
    assert_equal(200, st);
    if phash(&b1) == phash(&b2) {
        panic!("double-encoded must differ from single-encoded in transparent proxy");
    }
}

#[tokio::test]
async fn test_query_percent_encoding_slash_in_value_equivalent() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_query_percent_encoding_slash_in_value_equivalent");
    let base = cache_addr().await;
    let path = "/api/v1/client";
    
    use urlencoding;
    let q_enc = format!("user%5Bid%5D=288&domain=pe.example&language=en&picked=a%2Fb&ns={}", urlencoding::encode(&ns));
    
    use url::form_urlencoded;
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("user[id]", "288");
    serializer.append_pair("domain", "pe.example");
    serializer.append_pair("language", "en");
    serializer.append_pair("picked", "a/b");
    serializer.append_pair("ns", &ns);
    let q_raw = serializer.finish();
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}?{}", base, path, q_enc), &headers).await);
    assert_equal(200, st);
    let (st, _, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}?{}", base, path, q_raw), &headers).await);
    assert_equal(200, st);
    if phash(&b1) != phash(&b2) {
        panic!("slash raw vs %2F must be equivalent");
    }
}

