// Cache proxy controller for main cache handler.

use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::info;
use sysinfo::System;

use crate::config::{Config, ConfigTrait};
use crate::dedlog;
use crate::http::header::filter_and_sort_request as filter_and_sort_headers;
use crate::http::query::filter_and_sort_request as filter_and_sort_queries;
use crate::http::render::renderer;
use crate::http::Controller;
use crate::http::{is_compression_enabled, panics_counter};
use crate::controller::metrics;
use crate::metrics as prom_metrics;
use crate::metrics::policy::Policy as LifetimePolicy;
use crate::model::{
    is_cache_rule_not_found_err, match_cache_rule, Response as ModelResponse,
};
use crate::db::Storage;
use crate::time;
use crate::traces;
use crate::upstream::actual_policy;
use crate::upstream::Upstream;

// Error constants
const ERR_MSG_INTERNAL_ERROR: &str = "internal error";
const ERR_MSG_UPSTREAM_INTERNAL_ERROR: &str = "upstream internal error";
const ERR_MSG_UPSTREAM_ERROR_WHILE_PROXYING: &str = "fetch upstream error while proxying";
const ERR_MSG_UPSTREAM_ERROR_WHILE_CACHE_PROXYING: &str =
    "fetch upstream error while cache-proxying";
const ERR_MSG_WRITE_ENTRY_TO_RESPONSE: &str = "write entry into response failed";

// Error types
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    // NeedRetryThroughProxy - is the marker error for the control the behavior 
    // when we need to retry a request from proxy.
    #[error("need retry through proxy")]
    NeedRetryThroughProxy,
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
        // Update metrics in real-time
        metrics::inc_total(1);

        // Extract request information
        let uri = request.uri();
        let path = uri.path();
        let path_bytes = path.as_bytes();

        // Extract headers
        // Note: HeaderName::to_string() returns lowercase, but we preserve original case via as_str()
        let mut request_headers = Vec::new();
        for (k, v) in request.headers() {
            if let Ok(v_str) = v.to_str() {
                // Use as_str() to get the original header name (axum normalizes to lowercase)
                // But for comparison, we use eq_ignore_ascii_case anyway
                let header_name = k.as_str().to_string();
                request_headers.push((header_name, v_str.to_string()));
            }
        }

        // Extract query string
        let query_str = uri.query().unwrap_or("");

        // Build request string representation for tracing
        let request_str = format!("{} {} {:?}", request.method(), uri, request.version());

        let tracing_enabled = traces::is_active_tracing();

        if tracing_enabled {
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

        #[derive(Copy, Clone)]
        enum PathKind {
            Cache,
            Proxy,
        }

        let mut path_kind = PathKind::Cache;

        // Handle request based on cache mode with fallback to proxy when needed.
        let result = if controller.cfg.is_enabled() {
            match controller
                .handle_through_cache(
                    path_bytes,
                    query_str,
                    &request_headers,
                    request.method().as_str(),
                    &request_str,
                )
                .await
            {
                Ok(ok) => Ok(ok),
                Err(CacheError::NeedRetryThroughProxy) => {
                    path_kind = PathKind::Proxy;
                    PROXIED.fetch_add(1, Ordering::Relaxed);
                    metrics::inc_proxied(1);
                    controller
                        .handle_through_proxy(
                            path,
                            query_str,
                            &request_headers,
                            request.method().as_str(),
                            &request_str,
                        )
                        .await
                }
                Err(err) => Err(err),
            }
        } else {
            path_kind = PathKind::Proxy;
            PROXIED.fetch_add(1, Ordering::Relaxed);
            metrics::inc_proxied(1);
            controller
                .handle_through_proxy(path, query_str, &request_headers, request.method().as_str(), &request_str)
                .await
        };

        let elapsed = start.elapsed().as_nanos() as i64;

        let (response, cache_hit, cache_key_attr) = match result {
            Ok((resp, hit, _is_error, key)) => (resp, hit, key),
            Err(err) => {
                DURATION.fetch_add(elapsed, Ordering::Relaxed);
                ERROR_DURATION.fetch_add(elapsed, Ordering::Relaxed);
                ERRORED.fetch_add(1, Ordering::Relaxed);
                metrics::inc_errors(1);
                let status_code = StatusCode::SERVICE_UNAVAILABLE.as_u16();
                metrics::inc_status_code(status_code);

                // Set tracing attributes for error case
                if let Some(ref s) = span {
                    s.record(traces::ATTR_HTTP_STATUS_CODE_KEY, status_code);
                    s.record(traces::ATTR_CACHE_HIT, false);
                    s.record(traces::ATTR_CACHE_KEY, 0i64); // No cache key available on error
                    s.record(traces::ATTR_CACHE_IS_ERR, true);
                }

                return controller.respond_service_unavailable(&err, &request_str);
            }
        };

        let status_code = response.status().as_u16();

        // Get response size from content-length header or use 0
        let response_size = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        // Record status code metric
        metrics::inc_status_code(status_code);

        // Update duration metrics
        DURATION.fetch_add(elapsed, Ordering::Relaxed);
        match path_kind {
            PathKind::Cache => CACHE_DURATION.fetch_add(elapsed, Ordering::Relaxed),
            PathKind::Proxy => PROXY_DURATION.fetch_add(elapsed, Ordering::Relaxed),
        };

        // Set tracing span attributes after handling
        if let Some(ref s) = span {
            if matches!(path_kind, PathKind::Proxy) && !controller.cfg.is_enabled() {
                s.record(traces::ATTR_CACHE_PROXY, true);
            }
            s.record(traces::ATTR_CACHE_HIT, cache_hit);
            s.record(traces::ATTR_CACHE_IS_ERR, false);
            s.record(traces::ATTR_CACHE_KEY, cache_key_attr as i64);
            s.record(traces::ATTR_HTTP_STATUS_CODE_KEY, status_code);
            s.record(traces::ATTR_HTTP_RESPONSE_SIZE_KEY, response_size);
        }

        response
    }

    /// Handles request through cache (cache mode).
    async fn handle_through_cache(
        &self,
        path_bytes: &[u8],
        query_str: &str,
        request_headers: &[(String, String)],
        _method: &str,
        request_str: &str,
    ) -> Result<(Response, bool, bool, u64), CacheError> {
        // Attempts to find cache rule in config. Otherwise just proxy it.
        let rule = match match_cache_rule(&self.cfg, path_bytes) {
            Ok(r) => r, // Already Arc<Rule>, just clone the Arc
            Err(e) => {
                if is_cache_rule_not_found_err(&*e) {
                    return Err(CacheError::NeedRetryThroughProxy);
                }
                return Err(CacheError::Other(anyhow::anyhow!("{}", e)));
            }
        };

        // Extract forwarded_host from original headers BEFORE filtering.
        // This ensures X-Forwarded-Host and Host are available even if not in cache key whitelist.
        let headers_bytes_for_forwarded: Vec<(Vec<u8>, Vec<u8>)> = request_headers
            .iter()
            .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
            .collect();
        let forwarded_host = crate::upstream::proxy::forwarded_host_value_bytes(&headers_bytes_for_forwarded);
        
        let headers_bytes = filter_and_sort_headers(Some(&rule), request_headers);
        let queries_bytes = filter_and_sort_queries(Some(&rule), query_str);

        let request_entry = crate::model::Entry::new(rule.clone(), queries_bytes.as_ref(), headers_bytes.as_ref());

        let (cache_entry_opt, hit) = self.cache.get(&request_entry);

        if hit {
            if let Some(cache_entry) = cache_entry_opt {
                HITS.fetch_add(1, Ordering::Relaxed);
                metrics::inc_cache_hits(1);

                let cache_key = cache_entry.key();
                return match renderer::write_from_entry(&cache_entry) {
                    Ok(response) => Ok((response, true, false, cache_key)),
                    Err(e) => {
                dedlog::err(Some(e.as_ref()), Some(request_str), ERR_MSG_WRITE_ENTRY_TO_RESPONSE);
                        Err(CacheError::NeedRetryThroughProxy)
                    }
                };
            }
        }

        MISSES.fetch_add(1, Ordering::Relaxed);
        metrics::inc_cache_misses(1);

        let cache_key = request_entry.key();
        
        // Add forwarded_host to headers_bytes so it's available in request().
        // This ensures Host header is passed to upstream even if not in cache key whitelist.
        // Note: We clone headers_bytes because it was moved into request_entry above.
        let mut headers_bytes_with_host = headers_bytes.clone();
        if let Some(host_bytes) = forwarded_host {
            headers_bytes_with_host.push((b"host".to_vec(), host_bytes.to_vec()));
        }
        
        let upstream_resp = match self
            .upstream
            .request(&rule, queries_bytes.as_ref(), &headers_bytes_with_host)
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
            dedlog::err(Some(e.as_ref()), Some(request_str), ERR_MSG_UPSTREAM_ERROR_WHILE_CACHE_PROXYING);
                return Err(CacheError::Other(e));
            }
        };

        let mut refreshed_at = 0i64;
        if upstream_resp.status == 200 {
            let model_response = ModelResponse {
                status: upstream_resp.status,
                headers: upstream_resp.headers.clone(),
                body: upstream_resp.body.clone(),
            };

            request_entry.set_payload(&queries_bytes, &headers_bytes, &model_response);

            if self.cache.set(request_entry) {
                refreshed_at = time::unix_nano();
            }
        } else {
            self.log_on_err_status_code(upstream_resp.status, request_str);
        }

        let model_resp = ModelResponse {
            status: upstream_resp.status,
            headers: upstream_resp.headers.clone(),
            body: upstream_resp.body.clone(),
        };
        let response = renderer::write_from_response(&model_resp, refreshed_at);

        Ok((response, false, false, cache_key))
    }

    /// Handles request through proxy (proxy mode).
    async fn handle_through_proxy(
        &self,
        path: &str,
        query_str: &str,
        request_headers: &[(String, String)],
        method: &str,
        request_str: &str,
    ) -> Result<(Response, bool, bool, u64), CacheError> {
        let upstream_resp = match self
            .upstream
            .proxy_request(
                method,
                path,
                query_str,
                request_headers,
                None,
            )
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                // Use dedlog for error logging
                dedlog::err(Some(e.as_ref()), Some(request_str), ERR_MSG_UPSTREAM_ERROR_WHILE_PROXYING);
                return Err(CacheError::Other(e));
            }
        };

        self.log_on_err_status_code(upstream_resp.status, request_str);

        let model_resp = ModelResponse {
            status: upstream_resp.status,
            headers: upstream_resp.headers.clone(),
            body: upstream_resp.body.clone(),
        };
        let response = renderer::write_from_response(&model_resp, 0);
        Ok((response, false, false, 0))
    }

    /// Logs error on non-OK status codes.
    fn log_on_err_status_code(&self, code: u16, request_str: &str) {
        if code >= 500 {
            dedlog::err(None, Some(request_str), ERR_MSG_UPSTREAM_INTERNAL_ERROR);
            ERRORED.fetch_add(1, Ordering::Relaxed);
            metrics::inc_errors(1);
        }
    }

    /// Returns 503 Service Unavailable response and logs the error.
    fn respond_service_unavailable(&self, err: &dyn std::error::Error, request_str: &str) -> Response {
        // Use dedlog for error logging
        dedlog::err(Some(err), Some(request_str), ERR_MSG_INTERNAL_ERROR);

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

        let body = crate::http::render::templates::UNAVAILABLE_RESPONSE_BODY;

        Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header("content-length", body.len())
            .body(body.to_vec().into())
            .map(|mut resp| {
                *resp.headers_mut() = headers;
                resp
            })
            .unwrap_or_else(|e| {
                dedlog::err(Some(&e), Some(request_str), "attempt to write service unavailable response failed");
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
            
            // Initialize CPU monitoring
            let mut sys = System::new();
            sys.refresh_cpu();
            // Wait a bit for accurate CPU readings
            tokio::time::sleep(Duration::from_millis(100)).await;
            let num_cores = num_cpus::get() as f64;

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
                        // Panics counter is updated in real-time via inc_panics() in middleware
                        // when panics occur. We read it here only for logging.
                        let panics = panics_counter() as i64;
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
                        // Update gauges (absolute values that change over time)
                        metrics::set_cache_length(length as u64);
                        metrics::set_cache_memory(mem_usage as u64);
                        metrics::set_avg_response_time(avg_duration, cache_avg_duration, proxy_avg_duration, errors_avg_duration);
                        metrics::set_rps(rps);
                        
                        // Update CPU usage in cores
                        sys.refresh_cpu();
                        let cpu_usage_percent = sys.global_cpu_info().cpu_usage() as f64;
                        let cpu_usage_cores = (cpu_usage_percent / 100.0) * num_cores;
                        metrics::set_cpu_usage_cores(cpu_usage_cores);
                        
                        // Note: Counters (hits, misses, total, errors, proxied) are now updated
                        // in real-time when events occur. Local counters are reset here for
                        // rate calculation (RPS) and logging, but metrics remain accumulated.
                        // Status codes are also updated in real-time via inc_status_code().
                        // Panics counter is tracked separately in middleware and read here for logging.

                        if cfg.is_enabled() {
                            let hit_rate = if hits_num + misses_num > 0 {
                                (hits_num as f64 / (hits_num + misses_num) as f64) * 100.0
                            } else {
                                0.0
                            };
                            let err_rate = if total_num > 0 {
                                (errors_num as f64 / total_num as f64) * 100.0
                            } else {
                                0.0
                            };

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
                            let err_rate = if total_num > 0 {
                                (errors_num as f64 / total_num as f64) * 100.0
                            } else {
                                0.0
                            };

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
        router.route(
            "/*path",
            get({
                let controller = controller.clone();
                move |request: axum::extract::Request| {
                    let controller = controller.clone();
                    async move { Self::index(State(controller), request).await }
                }
            }),
        )
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
