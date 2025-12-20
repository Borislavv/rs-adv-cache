// Common test utilities for integration tests.

use std::collections::HashMap;
use std::time::Duration;

pub type H = HashMap<String, String>;

/// Creates a new namespace for test isolation.
pub fn new_namespace(test_name: &str) -> String {
    use hex;
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(test_name.as_bytes());
    let hash = hasher.finalize();
    let hash_str = hex::encode(&hash[..4]);
    format!("{}_{}", test_name.replace("/", "_"), hash_str)
}

/// Builds a URL with namespace and parameters.
pub fn with_ns(path: &str, ns: &str, params: &HashMap<String, String>) -> String {
    use url::form_urlencoded;
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (k, v) in params {
        serializer.append_pair(k, v);
    }
    serializer.append_pair("ns", ns);
    format!("{}?{}", path, serializer.finish())
}

/// Makes an HTTP request.
pub async fn do_request(
    method: &str,
    url: &str,
    headers: &H,
    body: Option<&[u8]>,
) -> Result<reqwest::Response, reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let mut request = match method {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        _ => panic!("unsupported method: {}", method),
    };

    for (k, v) in headers {
        request = request.header(k, v);
    }

    if let Some(body_data) = body {
        request = request.body(body_data.to_vec());
    }

    request.send().await
}

/// Makes an HTTP request and parses JSON response.
pub async fn do_json<T: serde::de::DeserializeOwned>(
    method: &str,
    url: &str,
    headers: &H,
) -> Result<
    (
        u16,
        HashMap<String, String>,
        Vec<u8>,
        Option<T>,
    ),
    reqwest::Error,
> {
    let resp = do_request(method, url, headers, None).await?;
    let status = resp.status().as_u16();

    // Convert headers to HashMap
    let mut header_map = HashMap::new();
    for (k, v) in resp.headers() {
        let k_str: String = k.as_str().to_string();
        if let Ok(v_str) = v.to_str() {
            header_map.insert(k_str, v_str.to_string());
        }
    }

    let body = resp.bytes().await?.to_vec();

    let parsed: Option<T> = if header_map
        .get("content-type")
        .map(|s| s.contains("json"))
        .unwrap_or(false)
        && !body.is_empty()
    {
        serde_json::from_slice(&body).ok()
    } else {
        None
    };

    Ok((status, header_map, body, parsed))
}

/// Assertions

/// Asserts that an error is None.
pub fn assert_ok<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|e| panic!("unexpected error: {}", e))
}

/// Asserts that two values are equal.
pub fn assert_equal<T: PartialEq + std::fmt::Debug>(want: T, got: T) {
    if want != got {
        panic!("want={:?} got={:?}", want, got);
    }
}

/// Computes SHA1 hash of bytes for test comparisons.
pub fn phash(b: &[u8]) -> String {
    use hex;
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(b);
    hex::encode(hasher.finalize())
}

/// Computes SHA1 hash of bytes (alias for phash).
pub fn hash(b: &[u8]) -> String {
    phash(b)
}

/// Extracts the Last-Updated-At header value from response headers.
/// 
/// Returns the header value as a String, or panics if the header is missing.
/// This is used to determine cache HIT/MISS: if Last-Updated-At stays the same,
/// it's a cache HIT; if it changes, it's a cache MISS.
pub fn get_updated_at(headers: &HashMap<String, String>) -> String {
    headers
        .get("last-updated-at")
        .or_else(|| headers.get("Last-Updated-At"))
        .expect("Last-Updated-At header must be present in response")
        .clone()
}

/// Sets global admin defaults to ensure deterministic test behavior.
/// 
/// This function sets safe defaults for admin toggles:
/// - Bypass: OFF (cache mode enabled)
/// - Admission: ON (admission control enabled)
/// 
/// Should be called at the start of tests that depend on specific admin state.
pub async fn set_global_defaults(base: &str) {
    use crate::support::{assert_ok, do_json, H};
    
    // Disable bypass (enable cache mode) - use /cache/bypass/off endpoint
    let client = reqwest::Client::new();
    let _ = client
        .get(&format!("{}/cache/bypass/off", base))
        .send()
        .await
        .unwrap();
    
    // Enable admission control
    #[derive(serde::Deserialize)]
    struct AdmissionResponse {
        #[serde(rename = "is_active")]
        _is_active: bool,
    }
    let _ = assert_ok(
        do_json::<AdmissionResponse>("GET", &format!("{}/advcache/admission/on", base), &H::new()).await,
    );
}
