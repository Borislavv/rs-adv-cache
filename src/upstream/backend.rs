use anyhow::{Context, Result};
use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tokio::time::timeout;
use tracing::{error, warn};

use crate::config::{Backend, Traces, Rule};
use crate::model::Entry;
use super::{Policy, Response, Upstream, actual_policy, change_policy};

#[allow(dead_code)]
const HTTPS: &str = "https";
#[allow(dead_code)]
const LIMIT_BUCKETS: usize = 256;
const BURST_PERCENT: u32 = 10;


#[derive(Debug, thiserror::Error)]
pub enum UpstreamError {
    #[error("backend is down")]
    BackendIsDown,
    #[error("backend is too busy")]
    BackendIsTooBusy,
    #[error("bad status code")]
    NotHealthyStatusCode,
    #[error("invalid upstream status code")]
    RefreshUpstreamBadStatusCode,
}

/// Backend implementation for upstream requests.
pub struct BackendImpl {
    #[allow(dead_code)]
    id: Vec<u8>,
    shutdown_token: CancellationToken,
    cfg: Backend,
    #[allow(dead_code)]
    trace_cfg: Option<Traces>,
    client: reqwest::Client,
    await_rl: Arc<RateLimiter<governor::state::direct::NotKeyed, governor::state::InMemoryState, governor::clock::DefaultClock>>,
    deny_rl: Arc<RateLimiter<governor::state::direct::NotKeyed, governor::state::InMemoryState, governor::clock::DefaultClock>>,
    alive: Arc<AtomicBool>,
}

impl BackendImpl {
    /// Creates a new instance of Backend.
    pub fn new(
        shutdown_token: CancellationToken,
        cfg: Option<Backend>,
        trace_cfg: Option<Traces>,
    ) -> Result<Arc<Self>> {
        let cfg = cfg.context("backend configuration is required")?;
        
        let rate = cfg.rate.unwrap_or(15000) as u32;
        let burst = (rate / 100 * BURST_PERCENT).max(1);
        
        // Create rate limiters
        let await_quota = Quota::per_second(NonZeroU32::new(rate).unwrap());
        let await_rl = Arc::new(RateLimiter::direct(await_quota));
        
        let deny_quota = Quota::per_second(NonZeroU32::new(rate).unwrap())
            .allow_burst(NonZeroU32::new(burst).unwrap());
        let deny_rl = Arc::new(RateLimiter::direct(deny_quota));

        // Create HTTP client
        let client = reqwest::Client::builder()
            .timeout(cfg.timeout.unwrap_or(Duration::from_secs(10)))
            .build()
            .context("Failed to create HTTP client")?;

        let policy = Policy::from_str(cfg.policy.as_deref().unwrap_or("deny"))
            .unwrap_or(Policy::Deny);
        change_policy(policy)?;

        let backend = Arc::new(Self {
            id: cfg.id_bytes.as_ref()
                .map(|b| b.clone())
                .unwrap_or_else(|| cfg.id.as_deref().unwrap_or("default").as_bytes().to_vec()),
            shutdown_token: shutdown_token.clone(),
            cfg,
            trace_cfg,
            client,
            await_rl,
            deny_rl,
            alive: Arc::new(AtomicBool::new(true)),
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
            warn!("clients pool is upped for upstream (to={})", 
                self.cfg.host.as_deref().unwrap_or("unknown"));
        } else {
            error!("clients pool is down for upstream (to={})", 
                self.cfg.host.as_deref().unwrap_or("unknown"));
        }
    }

    /// Gets the base URL for the backend.
    fn base_url(&self) -> String {
        let scheme = self.cfg.scheme.as_deref().unwrap_or("http");
        let host = self.cfg.host.as_deref().unwrap_or("localhost");
        format!("{}://{}", scheme, host)
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
            return Err(UpstreamError::BackendIsDown.into());
        }

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
        _trace_ctx: CancellationToken,
    ) -> Result<Response> {
        self.throttle().await?;

        let base_url = self.base_url();
        let path = rule.path.as_deref().unwrap_or("/");
        let mut url = format!("{}{}", base_url, path);

        // Build query string
        if !queries.is_empty() {
            let query_parts: Vec<String> = queries
                .iter()
                .map(|(k, v)| {
                    format!("{}={}", 
                        String::from_utf8_lossy(k),
                        urlencoding::encode(&String::from_utf8_lossy(v)))
                })
                .collect();
            url = format!("{}?{}", url, query_parts.join("&"));
        }

        // Build headers map for sanitization
        let mut header_map = axum::http::HeaderMap::new();
        for (key, value) in headers {
            if let (Ok(k), Ok(v)) = (
                axum::http::HeaderName::try_from(key.as_slice()),
                axum::http::HeaderValue::from_bytes(value),
            ) {
                header_map.insert(k, v);
            }
        }

        // Sanitize hop-by-hop headers from request
        crate::upstream::sanitize::sanitize_hop_by_hop_request_headers(&mut header_map);

        // Note: proxy_forwarded_host is not needed here as this is for cache request method
        // which doesn't use X-Forwarded-Host from original client request

        // Build request
        let mut request = self.client.get(&url);

        // Add sanitized headers
        for (key, value) in header_map.iter() {
            if let Ok(value_str) = value.to_str() {
                request = request.header(key.as_str(), value_str);
            }
        }

        // Execute request
        let timeout_duration = self.get_timeout(false);
        let response = timeout(timeout_duration, request.send())
            .await
            .context("Request timeout")?
            .context("Request failed")?;

        let status = response.status().as_u16();
        
        // Sanitize response headers
        let mut response_headers_map = axum::http::HeaderMap::new();
        for (key, value) in response.headers() {
            if let Ok(value_str) = value.to_str() {
                if let (Ok(name), Ok(val)) = (
                    axum::http::HeaderName::try_from(key.as_str().as_bytes()),
                    axum::http::HeaderValue::from_str(value_str),
                ) {
                    response_headers_map.insert(name, val);
                }
            }
        }
        crate::upstream::sanitize::sanitize_hop_by_hop_response_headers(&mut response_headers_map);
        
        // Convert sanitized headers to Vec
        let mut response_headers = Vec::new();
        for (key, value) in response_headers_map.iter() {
            if let Ok(value_str) = value.to_str() {
                response_headers.push((key.to_string(), value_str.to_string()));
            }
        }

        // Apply rule-based sanitization
        crate::upstream::sanitize::sanitize_response_headers_by_rule(Some(rule), &mut response_headers_map);
        
        // Rebuild response_headers after rule sanitization
        response_headers.clear();
        for (key, value) in response_headers_map.iter() {
            if let Ok(value_str) = value.to_str() {
                response_headers.push((key.to_string(), value_str.to_string()));
            }
        }

        let body = response.bytes().await
            .context("Failed to read response body")?
            .to_vec();

        Ok(Response::new(status, response_headers, body))
    }

    async fn proxy_request(
        &self,
        method: &str,
        path: &str,
        query: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
        _trace_ctx: CancellationToken,
    ) -> Result<Response> {
        self.throttle().await?;

        let base_url = self.base_url();
        let mut url = format!("{}{}", base_url, path);
        if !query.is_empty() {
            url = format!("{}?{}", url, query);
        }

        // Build source headers map from input headers (for proxy_forwarded_host)
        let mut src_header_map = axum::http::HeaderMap::new();
        for (key, value) in headers {
            if let (Ok(name), Ok(val)) = (
                axum::http::HeaderName::try_from(key.as_bytes()),
                axum::http::HeaderValue::from_str(value),
            ) {
                src_header_map.insert(name, val);
            }
        }

        // Build destination headers map for outgoing request
        let mut header_map = src_header_map.clone();

        // Apply proxy_forwarded_host - sets Host header from X-Forwarded-Host or Host in source
        crate::upstream::proxy::proxy_forwarded_host(&mut header_map, &src_header_map);

        // Sanitize hop-by-hop headers from request
        crate::upstream::sanitize::sanitize_hop_by_hop_request_headers(&mut header_map);

        // Build request based on method
        let mut request = match method {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "PUT" => self.client.put(&url),
            "DELETE" => self.client.delete(&url),
            _ => self.client.get(&url),
        };

        // Add sanitized headers
        for (key, value) in header_map.iter() {
            if let Ok(value_str) = value.to_str() {
                request = request.header(key.as_str(), value_str);
            }
        }

        // Add body if present
        if let Some(body_data) = body {
            request = request.body(body_data.to_vec());
        }

        // Execute request
        let timeout_duration = self.get_timeout(false);
        let response = timeout(timeout_duration, request.send())
            .await
            .context("Request timeout")?
            .context("Request failed")?;

        let status = response.status().as_u16();
        
        // Sanitize response headers
        let mut response_headers_map = axum::http::HeaderMap::new();
        for (key, value) in response.headers() {
            if let Ok(value_str) = value.to_str() {
                if let (Ok(name), Ok(val)) = (
                    axum::http::HeaderName::try_from(key.as_str().as_bytes()),
                    axum::http::HeaderValue::from_str(value_str),
                ) {
                    response_headers_map.insert(name, val);
                }
            }
        }
        crate::upstream::sanitize::sanitize_hop_by_hop_response_headers(&mut response_headers_map);
        
        // Convert sanitized headers to Vec
        let mut response_headers = Vec::new();
        for (key, value) in response_headers_map.iter() {
            if let Ok(value_str) = value.to_str() {
                response_headers.push((key.to_string(), value_str.to_string()));
            }
        }

        let body = response.bytes().await
            .context("Failed to read response body")?
            .to_vec();

        Ok(Response::new(status, response_headers, body))
    }

    async fn refresh(&self, entry: &mut Entry) -> Result<()> {
        use crate::upstream::trace;

        // Decode request payload from entry
        let request_payload = entry.request_payload()
            .context("failed to decode request payload for refresh")?;

        let queries = &request_payload.queries;
        let headers = &request_payload.headers;

        // Start tracing span
        let span = trace::start_refresh_span_context(entry);
        let _guard = span.enter();

        // Make request to upstream
        let rule = entry.rule.clone();
        let trace_ctx = CancellationToken::new();
        let resp = self.request(
            &rule,
            queries,
            headers,
            trace_ctx,
        ).await.map_err(|e| {
            trace::record_error_in_span(&span, e.as_ref());
            e
        }).context("failed to fetch new payload while refreshing")?;

        // Check status code
        if resp.status != 200 {
            return Err(UpstreamError::RefreshUpstreamBadStatusCode.into());
        }

        // Update entry payload and timestamps
        entry.set_payload(queries, headers, &resp);
        entry.touch_refreshed_at();
        entry.clear_refresh_queued();

        Ok(())
    }

    async fn is_healthy(&self) -> Result<()> {
        let healthcheck_path = self.cfg.healthcheck.as_deref()
            .unwrap_or("/healthcheck");
        let base_url = self.base_url();
        let url = format!("{}{}", base_url, healthcheck_path);

        let timeout_duration = self.cfg.timeout.unwrap_or(Duration::from_secs(10));
        let response = timeout(timeout_duration, self.client.get(&url).send())
            .await
            .context("Health check timeout")?
            .context("Health check failed")?;

        if response.status().as_u16() != 200 {
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

