// Test upstream server for integration tests.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Allowed query keys for normalization.
const ALLOWED_QUERY_KEYS: &[&str] = &[
    "user[id]",
    "domain",
    "language",
    "picked",
    "timezone",
    "ns", // test namespace
];

/// Normalizes allowed query parameters.
fn normalize_allowed_query(raw: &str) -> String {
    let parsed: HashMap<String, Vec<String>> = url::form_urlencoded::parse(raw.as_bytes())
        .into_owned()
        .fold(HashMap::new(), |mut acc, (k, v)| {
            acc.entry(k).or_insert_with(Vec::new).push(v);
            acc
        });
    
    let mut allowed: Vec<_> = parsed
        .iter()
        .filter(|(k, _)| ALLOWED_QUERY_KEYS.contains(&k.as_str()))
        .collect();
    
    allowed.sort_by_key(|(k, _)| *k);
    
    let mut query_parts = Vec::new();
    for (k, values) in allowed {
        for v in values {
            query_parts.push(format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)));
        }
    }
    query_parts.join("&")
}

/// Computes the counter key from request.
fn compute_key(path: &str, query: &str, accept_encoding: &str) -> (String, String, String) {
    let ae_raw = accept_encoding.to_string();
    let ae_norm = if accept_encoding.to_lowercase().contains("gzip") {
        "gzip"
    } else {
        "identity"
    };
    let key = format!("{}?{}||{}", path, query, ae_norm);
    (key, ae_raw, ae_norm.to_string())
}

/// Derives a number from query string for payload generation.
fn derive_num_from_query(raw: &str) -> i32 {
    let norm = normalize_allowed_query(raw);
    let parsed: HashMap<String, Vec<String>> = url::form_urlencoded::parse(raw.as_bytes())
        .into_owned()
        .fold(HashMap::new(), |mut acc, (k, v)| {
            acc.entry(k).or_insert_with(Vec::new).push(v);
            acc
        });
    
    if let Some(user_id) = parsed.get("user[id]").and_then(|v| v.first()) {
        let mut sum = 0;
        for ch in user_id.chars() {
            if ch.is_ascii_digit() {
                sum = (sum * 10 + ch.to_digit(10).unwrap() as i32) % 100000;
            }
        }
        if sum > 0 {
            return sum;
        }
    }
    
    // Fallback: hash of normalized string
    let mut h = 0;
    for &byte in norm.as_bytes() {
        h = ((h * 33) + byte as i32) & 0xFFFF;
    }
    if h == 0 {
        h = 1;
    }
    h
}

/// Upstream counters for tracking requests.
pub struct UpstreamCounters {
    hits: Arc<Mutex<HashMap<String, i64>>>,
}

impl UpstreamCounters {
    fn new() -> Self {
        Self {
            hits: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    fn inc(&self, key: String) {
        let mut hits = self.hits.lock().unwrap();
        *hits.entry(key).or_insert(0) += 1;
    }
    
    pub fn snapshot(&self) -> HashMap<String, i64> {
        self.hits.lock().unwrap().clone()
    }
    
    pub fn get(&self, key: &str) -> i64 {
        *self.hits.lock().unwrap().get(key).unwrap_or(&0)
    }
}

impl Clone for UpstreamCounters {
    fn clone(&self) -> Self {
        Self {
            hits: self.hits.clone(),
        }
    }
}

/// Upstream test server.
pub struct UpstreamServer {
    addr: String,
    counter: UpstreamCounters,
    handle: JoinHandle<()>,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

impl UpstreamServer {
    /// Starts the upstream server.
    pub async fn start() -> Self {
        let counter = UpstreamCounters::new();
        let counter_for_handler = counter.clone();
        
        let handler = move |req: Request| {
            let counter = counter_for_handler.clone();
            async move {
                let path = req.uri().path().to_string();
                let query = req.uri().query().unwrap_or("");
                let accept_encoding = req
                    .headers()
                    .get("accept-encoding")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                
                let (ctr_key, ae_raw, ae) = compute_key(&path, query, &accept_encoding);
                counter.inc(ctr_key.clone());
                
                let n = derive_num_from_query(query);
                let payload = json!({
                    "title": format!("{} title", n),
                    "description": format!("{} description", n),
                    "echo": {
                        "path": path,
                        "query": normalize_allowed_query(query),
                        "ae": ae
                    }
                });
                
                let body = serde_json::to_vec(&payload).unwrap();
                
                let mut headers = HeaderMap::new();
                headers.insert("content-type", "application/json".parse().unwrap());
                headers.insert("x-up-key", ctr_key.parse().unwrap());
                headers.insert("x-up-ae", ae.parse().unwrap());
                headers.insert("x-up-ae-raw", ae_raw.parse().unwrap());
                
                if ae == "gzip" {
                    headers.insert("content-encoding", "gzip".parse().unwrap());
                    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                    use std::io::Write;
                    encoder.write_all(&body).unwrap();
                    let compressed = encoder.finish().unwrap();
                    (StatusCode::OK, headers, compressed).into_response()
                } else {
                    (StatusCode::OK, headers, body).into_response()
                }
            }
        };
        
        let router = Router::new()
            .route("/healthcheck", get(|| async { "ok" }))
            .route("/api/v1/user", axum::routing::any(handler.clone()))
            .route("/api/v1/client", axum::routing::any(handler.clone()))
            .route("/api/v1/buyer", axum::routing::any(handler.clone()))
            .route("/api/v1/customer", axum::routing::any(handler));
        
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let addr_str = format!("127.0.0.1:{}", addr.port());
        
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        
        let handle = tokio::spawn(async move {
            let server = axum::serve(listener, router);
            tokio::select! {
                _ = server => {},
                _ = shutdown_rx => {},
            }
        });
        
        // Wait for server to be ready
        wait_http_ready(&format!("http://{}/healthcheck", addr_str)).await;
        
        Self {
            addr: addr_str,
            counter,
            handle,
            shutdown: shutdown_tx,
        }
    }
    
    pub fn addr(&self) -> &str {
        &self.addr
    }
    
    pub fn counter(&self) -> &UpstreamCounters {
        &self.counter
    }
    
    /// Closes the upstream server.
    pub async fn close(self) {
        let _ = self.shutdown.send(());
        self.handle.abort();
    }
}

/// Waits for HTTP server to be ready.
async fn wait_http_ready(url: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        if let Ok(resp) = reqwest::get(url).await {
            if resp.status().is_success() {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("http not ready: {}", url);
}

