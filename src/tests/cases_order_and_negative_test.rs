// Integration tests for query order insensitivity and negative whitelist changes.

#[path = "support/mod.rs"]
mod support;

use std::collections::HashMap;
use support::{assert_equal, assert_ok, do_json, new_namespace, H, cache_addr, init_test_harness, hash};

fn build_raw_query(order: &[&str], vals: &HashMap<String, String>) -> String {
    use urlencoding;
    let mut parts = Vec::new();
    for k in order {
        if let Some(v) = vals.get(*k) {
            parts.push(format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)));
        }
    }
    parts.join("&")
}

#[tokio::test]
async fn test_query_order_insensitive() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_query_order_insensitive");
    let base = cache_addr().await;
    let path = "/api/v1/user";
    
    let mut vals = HashMap::new();
    vals.insert("user[id]".to_string(), "5001".to_string());
    vals.insert("domain".to_string(), "order.example".to_string());
    vals.insert("language".to_string(), "en".to_string());
    vals.insert("ns".to_string(), ns.clone());
    
    let ord1 = vec!["user[id]", "domain", "language", "ns"];
    let ord2 = vec!["language", "ns", "domain", "user[id]"];
    let raw1 = build_raw_query(&ord1, &vals);
    let raw2 = build_raw_query(&ord2, &vals);
    
    let u1 = format!("{}?{}", path, raw1);
    let u2 = format!("{}?{}", path, raw2);
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    
    // identity
    let (st, _, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u1), &headers).await);
    assert_equal(200, st);
    let (st, _, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u2), &headers).await);
    assert_equal(200, st);
    if hash(&b1) != hash(&b2) {
        panic!("same whitelisted params with different order must produce identical body");
    }
    
    // gzip
    headers.insert("Accept-Encoding".to_string(), "gzip".to_string());
    let (st, _, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u1), &headers).await);
    assert_equal(200, st);
    let (st, _, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u2), &headers).await);
    assert_equal(200, st);
    if hash(&b1) != hash(&b2) {
        panic!("gzip: same whitelisted params with different order must produce identical body");
    }
}

#[tokio::test]
async fn test_whitelist_negative_change_one_key_changes_body() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_whitelist_negative_change_one_key_changes_body");
    let base = cache_addr().await;
    let path = "/api/v1/user";
    
    let mut base_vals = HashMap::new();
    base_vals.insert("user[id]".to_string(), "7001".to_string());
    base_vals.insert("domain".to_string(), "neg.example".to_string());
    base_vals.insert("language".to_string(), "en".to_string());
    base_vals.insert("picked".to_string(), "A".to_string());
    base_vals.insert("timezone".to_string(), "UTC".to_string());
    base_vals.insert("ns".to_string(), ns.clone());
    
    let order = vec!["user[id]", "domain", "language", "picked", "timezone", "ns"];
    let u_base = format!("{}?{}", path, build_raw_query(&order, &base_vals));
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b0, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_base), &headers).await);
    assert_equal(200, st);
    let h0 = hash(&b0);
    
    let changes = vec![
        ("user[id]", "7002"),
        ("domain", "neg2.example"),
        ("language", "ru"),
        ("picked", "B"),
        ("timezone", "Europe/Amsterdam"),
    ];
    
    for (key, val) in changes {
        let mut vals = base_vals.clone();
        vals.insert(key.to_string(), val.to_string());
        let u = format!("{}?{}", path, build_raw_query(&order, &vals));
        let (st, _, b, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await);
        assert_equal(200, st);
        if hash(&b) == h0 {
            panic!("changing whitelisted key {:?} must change body", key);
        }
    }
}

