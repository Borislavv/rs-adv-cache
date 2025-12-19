use anyhow::{Context, Result};
use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};

use super::{actual_policy, change_policy, Policy, Response, Upstream};
use crate::config::{Backend, Rule};
use crate::model::Entry;
use crate::upstream::trace as upstream_trace;
use crate::upstream::proxy;

// Determines how fast will be the burst of requests within a second
const BURST_PERCENT: u32 = 10;

#[derive(Debug, thiserror::Error)]
pub enum UpstreamError {
    #[error("backend is down")]
    BackendIsDown,
    #[error("backend is too busy")]
    BackendIsTooBusy,
    #[error("bad status code")]
    NotHealthyStatusCode,
}

/// Backend implementation for upstream requests.
pub struct BackendImpl {
    shutdown_token: CancellationToken,
    cfg: Backend,
    client: crate::http::client::HyperClient,
    await_rl: Arc<
        RateLimiter<
            governor::state::direct::NotKeyed,
            governor::state::InMemoryState,
            governor::clock::DefaultClock,
        >,
    >,
    deny_rl: Arc<
        RateLimiter<
            governor::state::direct::NotKeyed,
            governor::state::InMemoryState,
            governor::clock::DefaultClock,
        >,
    >,
    alive: Arc<AtomicBool>,
    connection_semaphore: Arc<Semaphore>,
}

impl BackendImpl {
    /// Creates a new instance of Backend.
    pub fn new(
        shutdown_token: CancellationToken,
        cfg: Option<Backend>,
    ) -> Result<Arc<Self>> {
        let cfg = cfg.context("backend configuration is required")?;

        let rate = cfg.rate.unwrap_or(15000) as u32;
        let burst = (rate / 100 * BURST_PERCENT).max(1);

        let await_quota = Quota::per_second(NonZeroU32::new(rate).unwrap());
        let await_rl = Arc::new(RateLimiter::direct(await_quota));

        let deny_quota = Quota::per_second(NonZeroU32::new(rate).unwrap())
            .allow_burst(NonZeroU32::new(burst).unwrap());
        let deny_rl = Arc::new(RateLimiter::direct(deny_quota));

        use crate::http::client::create_client;
        let client = create_client();

        let max_concurrent_connections = cfg.concurrency.unwrap_or(4096);
        let connection_semaphore = Arc::new(Semaphore::new(max_concurrent_connections));

        let policy =
            Policy::from_str(cfg.policy.as_deref().unwrap_or("deny")).unwrap_or(Policy::Deny);
        change_policy(policy)?;

        let backend = Arc::new(Self {
            shutdown_token: shutdown_token.clone(),
            cfg,
            client,
            await_rl,
            deny_rl,
            alive: Arc::new(AtomicBool::new(true)),
            connection_semaphore,
        });

        // Start health observer
        let observer_backend = backend.clone();
        tokio::task::spawn(async move {
            observer_backend.observer().await;
        });

        Ok(backend)
    }

    /// Sets the health status of the backend.
    pub fn set_health(&self, up: bool) {
        let prev = self.alive.swap(up, Ordering::Relaxed);
        if prev == up {
            return;
        }
        if up {
            warn!(
                "clients pool is upped for upstream (to={})",
                self.cfg.host.as_deref().unwrap_or("unknown")
            );
        } else {
            error!(
                "clients pool is down for upstream (to={})",
                self.cfg.host.as_deref().unwrap_or("unknown")
            );
        }
    }

    /// Gets the base URL for the backend.
    fn base_url(&self) -> String {
        let scheme = self.cfg.scheme.as_deref().unwrap_or("http");
        let host = self.cfg.host.as_deref()
            .expect("backend.host must be configured");
        
        let normalized_host = if host == "localhost" || host.starts_with("localhost:") {
            if host.contains(':') {
                host.replace("localhost", "127.0.0.1")
            } else {
                "127.0.0.1".to_string()
            }
        } else {
            host.to_string()
        };
        
        format!("{}://{}", scheme, normalized_host)
    }

    /// Gets the timeout for requests.
    fn get_timeout(&self, use_max_timeout: bool) -> Duration {
        if use_max_timeout {
            self.cfg.max_timeout.unwrap_or(Duration::from_secs(60))
        } else {
            self.cfg.timeout.unwrap_or(Duration::from_secs(10))
        }
    }

    /// Throttles requests based on policy.
    async fn throttle(&self) -> Result<()> {
        if !self.alive.load(Ordering::Relaxed) {
            let host = self.cfg.host.as_deref().unwrap_or("unknown");
            tracing::warn!(
                host = %host,
                "Backend is marked as down, rejecting request"
            );
            return Err(UpstreamError::BackendIsDown.into());
        }

        let _permit = self.connection_semaphore.acquire().await
            .map_err(|_| anyhow::anyhow!("Connection semaphore closed"))?;

        match actual_policy() {
            Policy::Await => {
                // Wait for rate limiter
                self.await_rl.until_ready().await;
                Ok(())
            }
            Policy::Deny => {
                // Try to acquire token, fail if not available
                if self.deny_rl.check().is_ok() {
                    Ok(())
                } else {
                    Err(UpstreamError::BackendIsTooBusy.into())
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl Upstream for BackendImpl {
    async fn request(
        &self,
        rule: &Rule,
        queries: &[(Vec<u8>, Vec<u8>)],
        headers: &[(Vec<u8>, Vec<u8>)],
    ) -> Result<Response> {
        self.throttle().await?;

        let base_url = self.base_url();
        let path = rule.path.as_deref().unwrap_or("/");
        let url = if queries.is_empty() {
            format!("{}{}", base_url, path)
        } else {
            // Build query string with single allocation
            let mut url_with_query = String::with_capacity(base_url.len() + path.len() + queries.len() * 32);
            url_with_query.push_str(&base_url);
            url_with_query.push_str(path);
            url_with_query.push('?');
            
            // Build query string (encodes both key and value)
            for (i, (k, v)) in queries.iter().enumerate() {
                if i > 0 {
                    url_with_query.push('&');
                }
                url_with_query.push_str(&urlencoding::encode(&String::from_utf8_lossy(k)));
                url_with_query.push('=');
                url_with_query.push_str(&urlencoding::encode(&String::from_utf8_lossy(v)));
            }
            url_with_query
        };

        // Parse URL to Uri
        let uri: hyper::Uri = url.parse()
            .with_context(|| format!("Invalid URL: {}", url))?;

        // Extract forwarded host value (X-Forwarded-Host or Host) as bytes (no allocations)
        let forwarded_host = proxy::forwarded_host_value_bytes(headers);

        let request_str = format!("GET {}", url);

        let span = upstream_trace::start_request_span(rule, &request_str);

        // Filter hop-by-hop headers (Host is handled separately via forwarded_host)
        let filtered_headers = proxy::filter_hop_by_hop_headers_bytes(headers);
        
        // Convert filtered headers to String tuples (excluding Host - it's set after build())
        let mut request_headers: Vec<(String, String)> = Vec::new();
        for (key, value) in &filtered_headers {
            // Skip Host header - it will be set via forwarded_host after build()
            if key.eq_ignore_ascii_case(b"host") {
                continue;
            }
            let k = match String::from_utf8(key.clone()) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let v = match String::from_utf8(value.clone()) {
                Ok(s) => s,
                Err(_) => continue,
            };
            request_headers.push((k, v));
        }
        
        let request_headers_refs: Vec<(&str, &str)> = request_headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let timeout_duration = self.get_timeout(false);
        
        use crate::upstream::backend_hyper_impl::make_get_request;
        use crate::upstream::backend_headers::process_response_headers;
        match make_get_request(&self.client, uri, request_headers_refs, timeout_duration, forwarded_host).await {
            Ok((status, response_headers_map, body)) => {
                // Process headers directly from response (optimized)
                let response_headers = process_response_headers(&response_headers_map, Some(rule));
                
                let response_size = body.len();
                
                // Record response in span
                if let Some(ref span) = span {
                    upstream_trace::record_response_in_span(span, status, response_size);
                }

                Ok(Response::new(status, response_headers, body))
            }
            Err(e) => {
                // Record error in span
                if let Some(ref span) = span {
                    upstream_trace::record_error_in_span(span, e.as_ref());
                }
                Err(e).context("Request failed")
            }
        }
    }

    async fn proxy_request(
        &self,
        method: &str,
        path: &str,
        query: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<Response> {
        self.throttle().await?;

        let base_url = self.base_url();
        let mut url = format!("{}{}", base_url, path);
        if !query.is_empty() {
            url = format!("{}?{}", url, query);
        }

        // Parse URL to Uri
        let uri: hyper::Uri = url.parse()
            .with_context(|| format!("Invalid URL: {}", url))?;

        // Build request string for tracing
        let request_str = format!("{} {}", method, url);

        // Parse HTTP method
        let http_method = match method {
            "GET" => hyper::Method::GET,
            "POST" => hyper::Method::POST,
            "PUT" => hyper::Method::PUT,
            "DELETE" => hyper::Method::DELETE,
            _ => hyper::Method::GET,
        };

        // Convert headers to bytes format for forwarded_host_value_bytes
        let headers_bytes: Vec<(Vec<u8>, Vec<u8>)> = headers
            .iter()
            .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
            .collect();
        
        // Extract forwarded host value (X-Forwarded-Host or Host) as bytes (no allocations)
        let forwarded_host = proxy::forwarded_host_value_bytes(&headers_bytes);

        // Sanitize hop-by-hop headers from request
        let filtered_headers = proxy::filter_hop_by_hop_headers(headers);

        // Start upstream span (after proxyForwardedHost)
        let span = upstream_trace::start_proxy_request_span(path, &request_str);

        // Build request headers (excluding Host - it's set after build())
        let mut request_headers: Vec<(&str, &str)> = Vec::new();
        for (key, value) in &filtered_headers {
            // Skip Host header - it will be set via forwarded_host after build()
            if key.eq_ignore_ascii_case("host") {
                continue;
            }
            request_headers.push((key.as_str(), value.as_str()));
        }

        // Convert body to Bytes if present
        let body_bytes = body.map(|b| hyper::body::Bytes::from(b.to_vec()));

        let timeout_duration = self.get_timeout(false);
        
        use crate::upstream::backend_hyper_impl::make_method_request;
        match make_method_request(&self.client, http_method, uri, request_headers, body_bytes, timeout_duration, forwarded_host).await {
            Ok((status, response_headers_map, body_bytes)) => {
                // Process headers directly from response (optimized)
                use crate::upstream::backend_headers::process_response_headers;
                let response_headers = process_response_headers(&response_headers_map, None);
                
                let response_size = body_bytes.len();
                
                // Record response in span
                if let Some(ref span) = span {
                    upstream_trace::record_response_in_span(span, status, response_size);
                }

                Ok(Response::new(status, response_headers, body_bytes))
            }
            Err(e) => {
                // Record error in span
                if let Some(ref span) = span {
                    upstream_trace::record_error_in_span(span, e.as_ref());
                }
                Err(e).context("Request failed")
            }
        }
    }

    async fn refresh(&self, entry: &Entry) -> Result<()> {
        use crate::dedlog;
        
        // Get request payload from entry
        let req_payload = entry.request_payload()
            .context("Failed to decode payload")?;
        
        let queries = &req_payload.queries;
        let headers = &req_payload.headers;
        
        // Start refresh span
        let span = upstream_trace::start_refresh_span_context(entry);
        
        let rule = entry.rule();
        let upstream_resp = match self.request(rule, queries, headers).await {
            Ok(r) => r,
            Err(e) => {
                // Record error in span
                if let Some(ref span) = span {
                    upstream_trace::record_error_in_span(span, e.as_ref());
                }
                let request_str = format!("GET {}", rule.path.as_deref().unwrap_or("/"));
                dedlog::err(Some(e.as_ref()), Some(&request_str), "failed to fetch new payload while refreshing");
                return Err(e);
            }
        };
        
        // Validate response status
        if upstream_resp.status != 200 {
            return Err(anyhow::anyhow!("invalid upstream status code: {}", upstream_resp.status));
        }
        
        use crate::model::Response as ModelResponse;
        let model_resp = ModelResponse {
            status: upstream_resp.status,
            headers: upstream_resp.headers,
            body: upstream_resp.body,
        };
        
        entry.set_payload(queries, headers, &model_resp);
        
        // Update timestamps
        entry.touch_refreshed_at();
        entry.clear_refresh_queued();
        
        Ok(())
    }

    async fn is_healthy(&self) -> Result<()> {
        let healthcheck_path = self.cfg.healthcheck.as_deref().unwrap_or("/healthz");
        let base_url = self.base_url();
        let url = format!("{}{}", base_url, healthcheck_path);
        
        let uri: hyper::Uri = url.parse()
            .with_context(|| format!("Invalid health check URL: {}", url))?;

        let timeout_duration = self.cfg.timeout.unwrap_or(Duration::from_secs(10));
        
        use crate::upstream::backend_hyper_impl::make_get_request;
        let (status, _, _) = make_get_request(&self.client, uri, Vec::new(), timeout_duration, None)
            .await
            .with_context(|| format!("Health check failed for URL: {}", url))?;


        if status != 200 {
            return Err(UpstreamError::NotHealthyStatusCode.into());
        }

        Ok(())
    }
}

/// Health observer that periodically checks backend health.
impl BackendImpl {
    async fn observer(&self) {
        const OK_CLOSE: u32 = 3; // after 3 in a row -> UP
        const FAIL_OPEN: u32 = 3; // after 3 in a row -> DOWN
        const BASE_PROBE: Duration = Duration::from_millis(500);
        const DOWN_PROBE: Duration = Duration::from_secs(1); // on DOWN throttling probes

        let mut interval = tokio::time::interval(BASE_PROBE);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut down = false;
        let mut fails = 0u32;
        let mut oks = 0u32;

        loop {
            tokio::select! {
                _ = self.shutdown_token.cancelled() => {
                    return;
                }
                _ = interval.tick() => {
                    match self.is_healthy().await {
                        Err(_) => {
                            // fail
                            oks = 0;
                            if !down {
                                fails += 1;
                                if fails >= FAIL_OPEN {
                                    down = true;
                                    self.set_health(false);
                                    fails = 0;
                                    interval = tokio::time::interval(DOWN_PROBE);
                                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                                }
                            }
                        }
                        Ok(_) => {
                            // success
                            fails = 0;
                            if down {
                                oks += 1;
                                if oks >= OK_CLOSE {
                                    down = false;
                                    self.set_health(true);
                                    oks = 0;
                                    interval = tokio::time::interval(BASE_PROBE);
                                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                                }
                            } else {
                                oks = 0;
                            }
                        }
                    }
                }
            }
        }
    }
}
