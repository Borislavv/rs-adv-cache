// Cache proxy controller for main cache handler.

use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::Response,
    Router,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Instant, Duration};
use tokio_util::sync::CancellationToken;
use tokio::time::interval;
use tracing::{error, info};

use crate::config::{Config, ConfigTrait};
use crate::http::Controller;
use crate::http::render::renderer;
use crate::http::query::filter_and_sort_request as filter_and_sort_queries;
use crate::http::header::filter_and_sort_request as filter_and_sort_headers;
use crate::storage::Storage;
use crate::upstream::Upstream;
use crate::model::{Entry, match_cache_rule, is_cache_rule_not_found_err, Response as ModelResponse};
use crate::time;
use crate::metrics as prom_metrics;
use crate::metrics::policy::Policy as LifetimePolicy;
use crate::traces;
use crate::http::{is_compression_enabled, panics_counter};
use crate::upstream::actual_policy;
use crate::safe;

// Error constants
// Removed unused constant ERR_MSG_ATTEMPT_TO_WRITE_503
const ERR_MSG_INTERNAL_ERROR: &str = "internal error";
const ERR_MSG_UPSTREAM_INTERNAL_ERROR: &str = "upstream internal error";
const ERR_MSG_UPSTREAM_ERROR_WHILE_PROXYING: &str = "fetch upstream error while proxying";
const ERR_MSG_UPSTREAM_ERROR_WHILE_CACHE_PROXYING: &str = "fetch upstream error while cache-proxying";
const ERR_MSG_WRITE_ENTRY_TO_RESPONSE: &str = "write entry into response failed";

// Error types
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("need retry through proxy")]
    NeedRetryThroughProxy,
    #[error("received an error status code")]
    #[allow(dead_code)]
    StatusCodeReceived,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// Metrics counters
static TOTAL: AtomicI64 = AtomicI64::new(0);
static HITS: AtomicI64 = AtomicI64::new(0);
static MISSES: AtomicI64 = AtomicI64::new(0);
static PROXIED: AtomicI64 = AtomicI64::new(0);
static ERRORED: AtomicI64 = AtomicI64::new(0);
static DURATION: AtomicI64 = AtomicI64::new(0);
static CACHE_DURATION: AtomicI64 = AtomicI64::new(0);
static PROXY_DURATION: AtomicI64 = AtomicI64::new(0);
static ERROR_DURATION: AtomicI64 = AtomicI64::new(0);

/// Handles cache API requests with read/write-through, error reporting, and metrics.
pub struct CacheProxyController {
    cfg: Arc<Config>,
    shutdown_token: CancellationToken,
    cache: Arc<dyn Storage>,
    upstream: Arc<dyn Upstream>,
}

impl CacheProxyController {
    /// Creates a new cache proxy controller.
    pub fn new(
        shutdown_token: CancellationToken,
        cfg: Config,
        cache: Arc<dyn Storage>,
        backend: Arc<dyn Upstream>,
    ) -> Self {
        let controller = Self {
            cfg: Arc::new(cfg),
            shutdown_token,
            cache,
            upstream: backend,
        };
        
        // Start metrics logger (runs every 5 seconds)
        controller.run_logger_metrics_writer();
        
        controller
    }

    /// Main HTTP handler for cache requests.
    async fn index(
        State(controller): State<Arc<Self>>,
        request: axum::extract::Request,
    ) -> Response {
        let start = Instant::now();
        TOTAL.fetch_add(1, Ordering::Relaxed);
        
        // Extract request information
        let uri = request.uri();
        let path = uri.path();
        let path_bytes = path.as_bytes();
        
        // Extract headers
        let mut request_headers = Vec::new();
        for (k, v) in request.headers() {
            if let Ok(v_str) = v.to_str() {
                request_headers.push((k.to_string(), v_str.to_string()));
            }
        }
        
        // Extract query string
        let query_str = uri.query().unwrap_or("");
        
        // Build request string representation for tracing
        let request_str = format!("{} {} {:?}", request.method(), uri, request.version());
        
        // Extract trace context from request headers and start tracing span if active
        let tracing_enabled = traces::is_active_tracing();
        
        // Extract trace context from headers if tracing is enabled
        // Note: With tracing-opentelemetry, context is automatically propagated through async boundaries
        // We don't need to explicitly attach the context here because tracing-opentelemetry
        // handles context propagation automatically when using tracing::span!
        if tracing_enabled {
            // Extract trace context from headers
            let trace_ctx = traces::extract(request.headers());
            // Attach context in synchronous block before any await
            // The guard will be dropped at end of block, but tracing-opentelemetry
            // will propagate the context through async boundaries automatically
            let _ctx_guard = trace_ctx.attach();
            // Context is now active and will be propagated by tracing-opentelemetry
        }
        
        // Create span using tracing API (integrates with OpenTelemetry via tracing-opentelemetry)
        // The span will automatically use the context that was attached above
        // We'll use the span directly for recording attributes without entering it
        // to avoid Send issues with guard across await points
        let span = if tracing_enabled {
            Some(tracing::span!(
                tracing::Level::INFO,
                "ingress",
                http.method = %request.method(),
                http.path = path,
                http.request = %request_str,
            ))
        } else {
            None
        };
        
        // Handle request based on cache mode
        // Trace context is automatically propagated through active span guard
        let result = if controller.cfg.is_enabled() {
            // Cache mode
            controller.handle_through_cache(path_bytes, query_str, &request_headers, request.method().as_str()).await
        } else {
            // Proxy mode
            PROXIED.fetch_add(1, Ordering::Relaxed);
            controller.handle_through_proxy(path, query_str, &request_headers, request.method().as_str()).await
        };
        
        let elapsed = start.elapsed().as_nanos() as i64;
        DURATION.fetch_add(elapsed, Ordering::Relaxed);
        
        let (response, is_hit, is_error, cache_key) = match &result {
            Ok((resp, hit, _err, key)) => (resp, *hit, false, *key),
            Err(_) => {
                ERROR_DURATION.fetch_add(elapsed, Ordering::Relaxed);
                ERRORED.fetch_add(1, Ordering::Relaxed);
                let status_code = StatusCode::SERVICE_UNAVAILABLE.as_u16();
                prom_metrics::inc_status_code(status_code);
                
                // Set tracing attributes for error case
                if let Some(ref s) = span {
                    s.record(traces::ATTR_HTTP_STATUS_CODE_KEY, status_code);
                    s.record(traces::ATTR_CACHE_HIT, false);
                    s.record(traces::ATTR_CACHE_KEY, 0i64); // No cache key available on error
                    s.record(traces::ATTR_CACHE_IS_ERR, true);
                }
                
                return match result {
                    Err(e) => controller.respond_service_unavailable(&e),
                    _ => unreachable!(),
                };
            }
        };
        
        let status_code = response.status().as_u16();
        
        // Get response size from content-length header or use 0
        let response_size = response.headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        
        // Record status code metric
        prom_metrics::inc_status_code(status_code);
        
        // Set tracing span attributes after handling
        if let Some(ref s) = span {
            if !controller.cfg.is_enabled() {
                s.record(traces::ATTR_CACHE_PROXY, true);
            }
            s.record(traces::ATTR_CACHE_HIT, is_hit);
            s.record(traces::ATTR_CACHE_IS_ERR, is_error);
            s.record(traces::ATTR_CACHE_KEY, cache_key as i64);
            s.record(traces::ATTR_HTTP_STATUS_CODE_KEY, status_code);
            s.record(traces::ATTR_HTTP_RESPONSE_SIZE_KEY, response_size);
        }
        
        match result {
            Ok((response, is_hit, is_error, _cache_key)) => {
                if is_error {
                    ERROR_DURATION.fetch_add(elapsed, Ordering::Relaxed);
                    ERRORED.fetch_add(1, Ordering::Relaxed);
                } else if is_hit {
                    CACHE_DURATION.fetch_add(elapsed, Ordering::Relaxed);
                } else if !controller.cfg.is_enabled() {
                    PROXY_DURATION.fetch_add(elapsed, Ordering::Relaxed);
                }
                response
            }
            Err(e) => {
                ERROR_DURATION.fetch_add(elapsed, Ordering::Relaxed);
                ERRORED.fetch_add(1, Ordering::Relaxed);
                controller.respond_service_unavailable(&e)
            }
        }
    }

    /// Handles request through cache (cache mode).
    async fn handle_through_cache(
        &self,
        path_bytes: &[u8],
        query_str: &str,
        request_headers: &[(String, String)],
        _method: &str,
    ) -> Result<(Response, bool, bool, u64), CacheError> {
        // Attempts to find cache rule in config. Otherwise just proxy it.
        let rule = match match_cache_rule(&self.cfg, path_bytes) {
            Ok(r) => Arc::new(r.clone()),
            Err(e) => {
                if is_cache_rule_not_found_err(&*e) {
                    return Err(CacheError::NeedRetryThroughProxy);
                }
                return Err(CacheError::Other(anyhow::anyhow!("{}", e)));
            }
        };

        // Convert headers to byte format for Entry
        let headers_bytes: Vec<(Vec<u8>, Vec<u8>)> = filter_and_sort_headers(Some(&*rule), request_headers)
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let queries_bytes: Vec<(Vec<u8>, Vec<u8>)> = filter_and_sort_queries(Some(&*rule), query_str)
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Build a lightweight entry with only keys for search.
        let mut request_entry = Entry::new(rule.clone(), &queries_bytes, &headers_bytes);

        // Search entry in cache.
        let (cache_entry_opt, hit) = self.cache.get(&request_entry);
        
        if hit {
            if let Some(cache_entry) = cache_entry_opt {
                // Record hit
                HITS.fetch_add(1, Ordering::Relaxed);

                // Write found entry in response (retry through proxy on error).
                let cache_key = cache_entry.key(); // Get key for span attributes
                return match renderer::write_from_entry(&cache_entry) {
                    Ok(response) => {
                        Ok((response, true, false, cache_key))
                    }
                    Err(e) => {
                        error!(
                            error = %e,
                            reason = ERR_MSG_WRITE_ENTRY_TO_RESPONSE,
                            "failed to write entry to response"
                        );
                        Err(CacheError::NeedRetryThroughProxy)
                    }
                }
            }
        }

        // Cache entry was not found, record miss.
        MISSES.fetch_add(1, Ordering::Relaxed);

        // Get cache key before moving request_entry
        let cache_key = request_entry.key();

        // Proxy request to upstream.
        // Trace context is automatically propagated through active span
        let upstream_resp = match self.upstream.request(
            &*rule,
            &queries_bytes,
            &headers_bytes,
            self.shutdown_token.clone(),
        ).await {
            Ok(resp) => resp,
            Err(e) => {
                error!(
                    error = %e,
                    reason = ERR_MSG_UPSTREAM_ERROR_WHILE_CACHE_PROXYING,
                    "upstream request failed"
                );
                return Err(CacheError::Other(e));
            }
        };

        // Validate upstream response and store it if possible.
        let mut refreshed_at = 0i64;
        let mut is_error_status = false;
        
        if upstream_resp.status == 200 {
            // Build and set up new cache entry payload.
            // Convert upstream response to model::Response format
            let model_response = ModelResponse {
                status: upstream_resp.status,
                headers: upstream_resp.headers.clone(),
                body: upstream_resp.body.clone(),
            };
            
            request_entry.set_payload(&queries_bytes, &headers_bytes, &model_response);

            // Trying to store entry in cache (it may be denied in order to admission policy).
            if self.cache.set(request_entry) {
                refreshed_at = time::unix_nano();
            }
        } else {
            // Handle upstream status code (write log on error).
            self.log_on_err_status_code(upstream_resp.status);
            if upstream_resp.status >= 500 {
                is_error_status = true;
            }
        }

        // Write fetched response.
        // Convert upstream::Response to model::Response
        let model_resp = ModelResponse {
            status: upstream_resp.status,
            headers: upstream_resp.headers.clone(),
            body: upstream_resp.body.clone(),
        };
        let response = renderer::write_from_response(&model_resp, refreshed_at);
        
        Ok((response, false, is_error_status, cache_key))
    }

    /// Handles request through proxy (proxy mode).
    async fn handle_through_proxy(
        &self,
        path: &str,
        query_str: &str,
        request_headers: &[(String, String)],
        method: &str,
    ) -> Result<(Response, bool, bool, u64), CacheError> {
        // Fetch missed data from upstream
        // Trace context is automatically propagated through active span
        let upstream_resp = match self.upstream.proxy_request(
            method,
            path,
            query_str,
            request_headers,
            None,
            self.shutdown_token.clone(),
        ).await {
            Ok(resp) => resp,
            Err(e) => {
                error!(
                    error = %e,
                    reason = ERR_MSG_UPSTREAM_ERROR_WHILE_PROXYING,
                    "proxy request failed"
                );
                return Err(CacheError::Other(e));
            }
        };
        
        let mut is_error_status = false;
        self.log_on_err_status_code(upstream_resp.status);
        if upstream_resp.status >= 500 {
            is_error_status = true;
        }
        
        // Write fetched response and return it
        // Convert upstream::Response to model::Response
        let model_resp = ModelResponse {
            status: upstream_resp.status,
            headers: upstream_resp.headers.clone(),
            body: upstream_resp.body.clone(),
        };
        let response = renderer::write_from_response(&model_resp, 0);
        // For proxy mode, we don't have a cache key, so use 0
        Ok((response, false, is_error_status, 0))
    }

    /// Logs error on non-OK status codes.
    fn log_on_err_status_code(&self, code: u16) {
        if code >= 500 {
            error!(
                reason = ERR_MSG_UPSTREAM_INTERNAL_ERROR,
                status_code = code,
                "upstream returned error status"
            );
            ERRORED.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Returns 503 Service Unavailable response and logs the error.
    fn respond_service_unavailable(&self, err: &dyn std::error::Error) -> Response {
        error!(
            error = %err,
            reason = ERR_MSG_INTERNAL_ERROR,
            "service unavailable"
        );
        
        let mut headers = HeaderMap::new();
        if let (Ok(name), Ok(value)) = (
            axum::http::header::HeaderName::try_from("x-error-reason".as_bytes()),
            HeaderValue::from_str(&err.to_string()),
        ) {
            headers.insert(name, value);
        }
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        
        const UNAVAILABLE_RESPONSE_BODY: &[u8] = b"{\"status\":503,\"error\":\"Service Unavailable\",\"message\":\"Sorry for that, please try again later or contact support.\"}";
        
        Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header("content-length", UNAVAILABLE_RESPONSE_BODY.len())
            .body(UNAVAILABLE_RESPONSE_BODY.to_vec().into())
            .map(|mut resp| {
                *resp.headers_mut() = headers;
                resp
            })
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Vec::new().into())
                    .unwrap()
            })
    }

    /// Runs metrics logger that periodically writes metrics to Prometheus.
    fn run_logger_metrics_writer(&self) {
        let cfg = self.cfg.clone();
        let cache = self.cache.clone();
        let shutdown_token = self.shutdown_token.clone();
        
        tokio::task::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut prev = time::now();
            
            loop {
                tokio::select! {
                    _ = shutdown_token.cancelled() => {
                        return;
                    }
                    _ = interval.tick() => {
                        // Swap counters to get values and reset them
                        let total_num = TOTAL.swap(0, Ordering::Relaxed);
                        let hits_num = HITS.swap(0, Ordering::Relaxed);
                        let misses_num = MISSES.swap(0, Ordering::Relaxed);
                        let proxied_num = PROXIED.swap(0, Ordering::Relaxed);
                        let errors_num = ERRORED.swap(0, Ordering::Relaxed);
                        let panics = panics_counter() as i64; // Note: panics_counter doesn't have swap, so we just read
                        let total_duration_num = DURATION.swap(0, Ordering::Relaxed);
                        let cache_duration_num = CACHE_DURATION.swap(0, Ordering::Relaxed);
                        let proxy_duration_num = PROXY_DURATION.swap(0, Ordering::Relaxed);
                        let error_duration_num = ERROR_DURATION.swap(0, Ordering::Relaxed);

                        // Calculate averages
                        let avg_duration = if total_num > 0 {
                            total_duration_num as f64 / total_num as f64
                        } else {
                            0.0
                        };

                        let cache_avg_duration = if hits_num + misses_num > 0 {
                            cache_duration_num as f64 / (hits_num + misses_num) as f64
                        } else {
                            0.0
                        };

                        let proxy_avg_duration = if proxied_num > 0 {
                            proxy_duration_num as f64 / proxied_num as f64
                        } else {
                            0.0
                        };

                        let errors_avg_duration = if errors_num > 0 {
                            error_duration_num as f64 / errors_num as f64
                        } else {
                            0.0
                        };

                        let now = time::now();
                        let elapsed = now.duration_since(prev).unwrap_or(Duration::from_secs(0));
                        let elapsed_secs = elapsed.as_secs_f64();
                        let rps = if elapsed_secs > 0.0 {
                            total_num as f64 / elapsed_secs
                        } else {
                            0.0
                        };
                        prev = time::now();

                        // Get cache statistics
                        let (mem_usage, length) = cache.stat();

                        // Set metrics
                        prom_metrics::set_backend_policy(actual_policy());
                        let lifetime_policy = LifetimePolicy::new_lifetime_policy(
                            cfg.lifetime().map(|l| l.is_remove_on_ttl.load(Ordering::Relaxed)).unwrap_or(false)
                        );
                        prom_metrics::set_lifetime_policy(lifetime_policy);
                        prom_metrics::set_is_bypass_active(cfg.is_enabled());
                        prom_metrics::set_is_compression_active(is_compression_enabled());
                        prom_metrics::set_is_admission_active(
                            cfg.admission().map(|a| a.is_enabled.load(Ordering::Relaxed)).unwrap_or(false)
                        );
                        prom_metrics::set_is_traces_active(traces::is_active_tracing());
                        prom_metrics::set_cache_length(length as u64);
                        prom_metrics::set_cache_memory(mem_usage as u64);
                        prom_metrics::add_hits(hits_num as u64);
                        prom_metrics::add_misses(misses_num as u64);
                        prom_metrics::add_total(total_num as u64);
                        prom_metrics::add_errors(errors_num as u64);
                        prom_metrics::add_proxied_num(proxied_num as u64);
                        prom_metrics::add_panics(panics as u64);
                        prom_metrics::set_avg_response_time(avg_duration, cache_avg_duration, proxy_avg_duration, errors_avg_duration);
                        prom_metrics::set_rps(rps);
                        prom_metrics::flush_status_code_counters();

                        if cfg.is_enabled() {
                            let hit_rate = safe::divide(hits_num, hits_num + misses_num) * 100.0;
                            let err_rate = safe::divide(errors_num, total_num) * 100.0;
                            
                            info!(
                                target = "cache-controller",
                                upstream_policy = ?actual_policy(),
                                served_total = total_num,
                                elapsed = ?elapsed,
                                rps = rps,
                                avg_duration = ?Duration::from_nanos(avg_duration as u64),
                                hit_rate = hit_rate,
                                err_rate = err_rate as i32,
                                hits = hits_num,
                                misses = misses_num,
                                errored = errors_num,
                                "ingress"
                            );
                        } else {
                            let err_rate = safe::divide(errors_num, total_num) * 100.0;
                            
                            info!(
                                target = "proxy-controller",
                                upstream_policy = ?actual_policy(),
                                served_total = total_num,
                                elapsed = ?elapsed,
                                rps = rps,
                                avg_duration = ?Duration::from_nanos(avg_duration as u64),
                                err_rate = err_rate as i32,
                                total = total_num,
                                proxied = proxied_num,
                                errored = errors_num,
                                "ingress"
                            );
                        }
                    }
                }
            }
        });
    }
}

impl Controller for CacheProxyController {
    fn add_route(&self, router: Router) -> Router {
        let controller = Arc::new(self.clone());
        // Use fallback handler for catch-all route (wildcard)
        // In axum, fallback handles all requests that don't match other routes
        router.fallback({
            let controller = controller.clone();
            move |request: axum::extract::Request| {
                let controller = controller.clone();
                async move {
                    Self::index(State(controller), request).await
                }
            }
        })
    }
}

impl Clone for CacheProxyController {
    fn clone(&self) -> Self {
        Self {
            cfg: self.cfg.clone(),
            shutdown_token: self.shutdown_token.clone(),
            cache: self.cache.clone(),
            upstream: self.upstream.clone(),
        }
    }
}
