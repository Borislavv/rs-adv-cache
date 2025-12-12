// Integration tests for cache key isolation.

#[path = "support/mod.rs"]
mod support;

use std::collections::HashMap;
use support::{assert_equal, assert_ok, do_json, new_namespace, with_ns, H, cache_addr, init_test_harness, hash};

#[derive(serde::Deserialize)]
struct RespPayload {
    #[serde(rename = "echo")]
    echo: EchoPayload,
}

#[derive(serde::Deserialize)]
struct EchoPayload {
    path: String,
    query: String,
    ae: String,
}

fn assert_body_matches(body: &[u8], got: &RespPayload, want_path: &str, want_raw_query: &str, want_ae: &str) {
    if got.echo.path != want_path {
        panic!("body mismatch: path want={:?} got={:?}\nraw={}", want_path, got.echo.path, String::from_utf8_lossy(body));
    }
    if got.echo.query != want_raw_query {
        panic!("body mismatch: query want={:?} got={:?}\nraw={}", want_raw_query, got.echo.query, String::from_utf8_lossy(body));
    }
    if got.echo.ae != want_ae {
        panic!("body mismatch: AE want={:?} got={:?}\nraw={}", want_ae, got.echo.ae, String::from_utf8_lossy(body));
    }
}

fn encode_query(m: &HashMap<String, String>) -> String {
    use url::form_urlencoded;
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    let mut keys: Vec<&String> = m.keys().collect();
    keys.sort();
    for k in keys {
        serializer.append_pair(k, m.get(k).unwrap());
    }
    serializer.finish()
}

#[tokio::test]
async fn test_key_isolation_path_variants() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_key_isolation_path_variants");
    let base = cache_addr().await;
    
    let mut q = HashMap::new();
    q.insert("user[id]".to_string(), "1001".to_string());
    q.insert("domain".to_string(), "example.org".to_string());
    q.insert("language".to_string(), "en".to_string());
    q.insert("ns".to_string(), ns.clone());
    
    let raw_q = encode_query(&q);
    let p1 = format!("/api/v1/user?{}", raw_q);
    let p2 = format!("/api/v1/client?{}", raw_q);
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    
    // Warm + assert
    let (st1, h1, b1, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}", base, p1), &headers).await);
    assert_equal(200, st1);
    let rp1: RespPayload = serde_json::from_slice(&b1).unwrap();
    assert_body_matches(&b1, &rp1, "/api/v1/user", &raw_q, "identity");
    
    let (st2, h2, b2, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}", base, p2), &headers).await);
    assert_equal(200, st2);
    let rp2: RespPayload = serde_json::from_slice(&b2).unwrap();
    assert_body_matches(&b2, &rp2, "/api/v1/client", &raw_q, "identity");
    
    // Cross-check: ensure headers Content-Type consistent
    let ct1 = h1.get("content-type");
    let ct2 = h2.get("content-type");
    if ct1.is_none() || ct2.is_none() || ct1 != ct2 {
        panic!("unexpected content-type mismatch: {:?} vs {:?}", ct1, ct2);
    }
    
    // Re-hit should not leak: request p1 after p2 and see p1 body
    let (_, _, b1b, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}", base, p1), &headers).await);
    let rp1b: RespPayload = serde_json::from_slice(&b1b).unwrap();
    assert_body_matches(&b1b, &rp1b, "/api/v1/user", &raw_q, "identity");
}

#[tokio::test]
async fn test_key_isolation_query_variants() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_key_isolation_query_variants");
    let base = cache_addr().await;
    
    let mut q1 = HashMap::new();
    q1.insert("user[id]".to_string(), "41".to_string());
    q1.insert("domain".to_string(), "example.org".to_string());
    q1.insert("language".to_string(), "en".to_string());
    q1.insert("ns".to_string(), ns.clone());
    
    let mut q2 = HashMap::new();
    q2.insert("user[id]".to_string(), "42".to_string());
    q2.insert("domain".to_string(), "example.org".to_string());
    q2.insert("language".to_string(), "en".to_string());
    q2.insert("ns".to_string(), ns.clone());
    
    let raw_q1 = encode_query(&q1);
    let raw_q2 = encode_query(&q2);
    
    let path = "/api/v1/user";
    let u1 = format!("{}?{}", path, raw_q1);
    let u2 = format!("{}?{}", path, raw_q2);
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    
    let (st, _, b, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}", base, u1), &headers).await);
    assert_equal(200, st);
    let r1: RespPayload = serde_json::from_slice(&b).unwrap();
    assert_body_matches(&b, &r1, path, &raw_q1, "identity");
    
    let (st, _, b, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}", base, u2), &headers).await);
    assert_equal(200, st);
    let r2: RespPayload = serde_json::from_slice(&b).unwrap();
    assert_body_matches(&b, &r2, path, &raw_q2, "identity");
    
    // Re-request u1; must still reflect rawQ1
    let (st, _, b, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}", base, u1), &headers).await);
    assert_equal(200, st);
    let r1b: RespPayload = serde_json::from_slice(&b).unwrap();
    assert_body_matches(&b, &r1b, path, &raw_q1, "identity");
}

#[tokio::test]
async fn test_key_isolation_header_accept_encoding() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_key_isolation_header_accept_encoding");
    let base = cache_addr().await;
    
    let mut q = HashMap::new();
    q.insert("user[id]".to_string(), "55".to_string());
    q.insert("domain".to_string(), "ae.example".to_string());
    q.insert("language".to_string(), "en".to_string());
    q.insert("ns".to_string(), ns.clone());
    
    let raw_q = encode_query(&q);
    let path = "/api/v1/user";
    let u = format!("{}?{}", path, raw_q);
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    
    // identity
    let (st, hdr, b, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}", base, u), &headers).await);
    assert_equal(200, st);
    let ri: RespPayload = serde_json::from_slice(&b).unwrap();
    assert_body_matches(&b, &ri, path, &raw_q, "identity");
    if let Some(ce) = hdr.get("content-encoding") {
        if !ce.is_empty() {
            panic!("identity must not set Content-Encoding, got {:?}", ce);
        }
    }
    
    // gzip
    headers.insert("Accept-Encoding".to_string(), "gzip".to_string());
    let (st, hdr, b, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}", base, u), &headers).await);
    assert_equal(200, st);
    let rg: RespPayload = serde_json::from_slice(&b).unwrap();
    assert_body_matches(&b, &rg, path, &raw_q, "gzip");
    if let Some(ce) = hdr.get("content-encoding") {
        if !ce.to_lowercase().contains("gzip") {
            panic!("gzip must set Content-Encoding=gzip, got {:?}", ce);
        }
    }
    
    // identity again — must not leak gzip body
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}", base, u), &headers).await);
    assert_equal(200, st);
    let ri2: RespPayload = serde_json::from_slice(&b).unwrap();
    assert_body_matches(&b, &ri2, path, &raw_q, "identity");
}

#[tokio::test]
async fn test_key_isolation_mixed_matrix_no_leakage() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_key_isolation_mixed_matrix_no_leakage");
    let base = cache_addr().await;
    
    let paths = vec!["/api/v1/user", "/api/v1/client"];
    let projects = vec!["401", "402"];
    let aes = vec!["identity", "gzip"];
    
    #[derive(Hash, PartialEq, Eq, Clone)]
    struct Key {
        path: String,
        query: String,
        ae: String,
    }
    
    struct Val {
        body_hash: String,
        payload: RespPayload,
    }
    
    let mut seen: HashMap<Key, Val> = HashMap::new();
    
    // Warm and record fingerprints
    for p in &paths {
        for pr in &projects {
            let mut q_map = HashMap::new();
            q_map.insert("user[id]".to_string(), pr.to_string());
            q_map.insert("domain".to_string(), "mx.example".to_string());
            q_map.insert("language".to_string(), "en".to_string());
            q_map.insert("ns".to_string(), ns.clone());
            let q = encode_query(&q_map);
            let u = format!("{}?{}", p, q);
            for ae in &aes {
                let mut headers = H::new();
                headers.insert("Accept-Encoding".to_string(), ae.to_string());
                let (_, _, b, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}", base, u), &headers).await);
                let rp: RespPayload = serde_json::from_slice(&b).unwrap();
                assert_body_matches(&b, &rp, p, &q, ae);
                let h = hash(&b);
                seen.insert(Key { path: p.to_string(), query: q.clone(), ae: ae.to_string() }, Val { body_hash: h, payload: rp });
            }
        }
    }
    
    // Shuffle order (deterministically by reversed sequences) and re-assert exact body mapping
    for i in (0..paths.len()).rev() {
        for j in (0..projects.len()).rev() {
            let mut q_map = HashMap::new();
            q_map.insert("user[id]".to_string(), projects[j].to_string());
            q_map.insert("domain".to_string(), "mx.example".to_string());
            q_map.insert("language".to_string(), "en".to_string());
            q_map.insert("ns".to_string(), ns.clone());
            let q = encode_query(&q_map);
            let u = format!("{}?{}", paths[i], q);
            for k in (0..aes.len()).rev() {
                let ae = aes[k];
                let mut headers = H::new();
                headers.insert("Accept-Encoding".to_string(), ae.to_string());
                let (_, _, b, _) = assert_ok(do_json::<RespPayload>("GET", &format!("{}{}", base, u), &headers).await);
                let rp: RespPayload = serde_json::from_slice(&b).unwrap();
                assert_body_matches(&b, &rp, paths[i], &q, ae);
                let h = hash(&b);
                let key = Key { path: paths[i].to_string(), query: q.clone(), ae: ae.to_string() };
                if let Some(want) = seen.get(&key) {
                    if h != want.body_hash {
                        panic!("leakage or mismatch for {:?}: want body {} got {}", key, want.body_hash, h);
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn test_accept_encoding_normalization() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_accept_encoding_normalization");
    let base = cache_addr().await;
    
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "9301".to_string());
    params.insert("domain".to_string(), "ae-norm.example".to_string());
    params.insert("language".to_string(), "en".to_string());
    
    let u = with_ns("/api/v1/user", &ns, &params);
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "gzip".to_string());
    
    // plain "gzip"
    let (st, hdr, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await);
    assert_equal(200, st);
    if let Some(ce) = hdr.get("content-encoding") {
        if !ce.to_lowercase().contains("gzip") {
            panic!("want gzip content-encoding, got {:?}", ce);
        }
    }
    
    // list with spaces and uppercase
    headers.insert("Accept-Encoding".to_string(), "GZIP, deflate, br".to_string());
    let (st, hdr, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers).await);
    assert_equal(200, st);
    if let Some(ce) = hdr.get("content-encoding") {
        if !ce.to_lowercase().contains("gzip") {
            panic!("want gzip content-encoding, got {:?}", ce);
        }
    }
    
    if hash(&b1) != hash(&b2) {
        panic!("AE normalization mismatch: gzip vs \"GZIP, deflate, br\" must map to same body");
    }
}

#[tokio::test]
async fn test_header_case_insensitivity() {
    init_test_harness().await.unwrap();
    
    let ns = new_namespace("test_header_case_insensitivity");
    let base = cache_addr().await;
    
    let mut params = HashMap::new();
    params.insert("user[id]".to_string(), "9302".to_string());
    params.insert("domain".to_string(), "header-case.example".to_string());
    params.insert("language".to_string(), "en".to_string());
    
    let u = with_ns("/api/v1/user", &ns, &params);
    
    let mut headers1 = H::new();
    headers1.insert("Accept-Encoding".to_string(), "identity".to_string());
    let (st, _, b1, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers1).await);
    assert_equal(200, st);
    
    let mut headers2 = H::new();
    headers2.insert("accept-encoding".to_string(), "identity".to_string());
    let (st, _, b2, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u), &headers2).await);
    assert_equal(200, st);
    
    if hash(&b1) != hash(&b2) {
        panic!("request header name case must not affect result");
    }
}

#[tokio::test]
async fn test_namespace_isolation_changes_body() {
    init_test_harness().await.unwrap();
    
    let base = cache_addr().await;
    let ns_a = format!("{}_A", new_namespace("test_namespace_isolation_changes_body"));
    let ns_b = format!("{}_B", new_namespace("test_namespace_isolation_changes_body"));
    
    let mut params_a = HashMap::new();
    params_a.insert("user[id]".to_string(), "9303".to_string());
    params_a.insert("domain".to_string(), "ns.example".to_string());
    params_a.insert("language".to_string(), "en".to_string());
    
    let mut params_b = HashMap::new();
    params_b.insert("user[id]".to_string(), "9303".to_string());
    params_b.insert("domain".to_string(), "ns.example".to_string());
    params_b.insert("language".to_string(), "en".to_string());
    
    let u_a = with_ns("/api/v1/user", &ns_a, &params_a);
    let u_b = with_ns("/api/v1/user", &ns_b, &params_b);
    
    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());
    
    let (st, _, b_a, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_a), &headers).await);
    assert_equal(200, st);
    let (st, _, b_b, _) = assert_ok(do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_b), &headers).await);
    assert_equal(200, st);
    
    if hash(&b_a) == hash(&b_b) {
        panic!("different namespaces must produce different body (no cross-namespace leakage)");
    }
}

