// Integration tests for bracket canonicalization.

use crate::support::{
    assert_equal, assert_ok, cache_addr, do_json, init_test_harness, new_namespace, H,
};
use sha1::{Digest, Sha1};

fn qhash(b: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(b);
    hex::encode(hasher.finalize())
}

#[tokio::test]
async fn test_brackets_literal_vs_encoded_same_body() {
    init_test_harness().await.unwrap();

    let ns = new_namespace("Test_Brackets_LiteralVsEncoded_SameBody");
    let base = cache_addr().await;
    let path = "/api/v1/user";

    // Same whitelisted fields; only difference is key encoding for user[id].
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("user[id]", "4242");
    serializer.append_pair("domain", "brackets.example");
    serializer.append_pair("language", "en");
    serializer.append_pair("ns", &ns);
    let u_lit = format!("{}?{}", path, serializer.finish());

    let q_enc = format!(
        "user%5Bid%5D=4242&domain=brackets.example&language=en&ns={}",
        urlencoding::encode(&ns)
    );
    let u_enc = format!("{}?{}", path, q_enc);

    let mut headers = H::new();
    headers.insert("Accept-Encoding".to_string(), "identity".to_string());

    let (st, _, b1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_lit), &headers).await,
    );
    assert_equal(200, st);
    let (st, _, b2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_enc), &headers).await,
    );
    assert_equal(200, st);
    assert_eq!(qhash(&b1), qhash(&b2), "identity bodies must match");

    headers.insert("Accept-Encoding".to_string(), "gzip".to_string());
    let (st, _, b1, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_lit), &headers).await,
    );
    assert_equal(200, st);
    let (st, _, b2, _) = assert_ok(
        do_json::<serde_json::Value>("GET", &format!("{}{}", base, u_enc), &headers).await,
    );
    assert_equal(200, st);
    assert_eq!(qhash(&b1), qhash(&b2), "gzip bodies must match");
}
