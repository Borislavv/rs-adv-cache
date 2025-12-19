use super::{CacheBox, Config};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Creates a new test configuration.
#[allow(dead_code)]
pub fn new_test_config() -> Config {
    use std::collections::{HashMap, HashSet};

    let mut cfg = Config {
        cache: CacheBox {
            env: super::TEST.to_string(),
            enabled: true,
            atomic_enabled: Arc::new(AtomicBool::new(true)),
            logs: Some(super::Logs {
                level: Some("debug".to_string()),
            }),
            runtime: Some(super::Runtime { num_cpus: 12 }),
            api: Some(super::Api {
                name: Some("adv_cache_test:8091".to_string()),
                port: Some("8091".to_string()),
            }),
            upstream: Some(super::Upstream {
                policy: Some("deny".to_string()),
                cluster: None,
                backend: Some(super::Backend {
                    id: Some("test-up".to_string()),
                    id_bytes: None,
                    enabled: true,
                    policy: None,
                    scheme: Some("http".to_string()),
                    scheme_bytes: None,
                    host: Some("localhost:8090".to_string()),
                    host_bytes: None,
                    rate: Some(2_000_000),
                    concurrency: Some(500_000),
                    timeout: Some(Duration::from_secs(5)),
                    max_timeout: Some(Duration::from_secs(60)),
                    use_max_timeout_header: None,
                    use_max_timeout_header_bytes: None,
                    healthcheck: Some("/healthz".to_string()),
                    healthcheck_bytes: None,
                    addr: None,
                    health_path: None,
                }),
            }),
            data: Some(super::Data {
                dump: Some(super::Dump {
                    enabled: false,
                    dir: Some("public/dump".to_string()),
                    name: Some("cache.dump".to_string()),
                    max_versions: Some(3),
                    gzip: false,
                    crc32_control: true,
                }),
                mock: Some(super::Mock {
                    enabled: false,
                    length: Some(100000),
                }),
            }),
            admission: Some(super::Admission {
                enabled: true,
                is_enabled: Arc::new(AtomicBool::new(true)),
                capacity: Some(100000),
                shards: Some(256),
                min_table_len_per_shard: Some(1024),
                sample_multiplier: Some(10),
                door_bits_per_counter: Some(12),
            }),
            compression: Some(super::Compression {
                enabled: true,
                level: Some(1), // BestSpeed compression level
            }),
            storage: Some(super::Storage {
                mode: Some("listing".to_string()),
                is_listing: true,
                size: 1_036_870_900,
                soft_memory_limit: 0,
                hard_memory_limit: 0,
                admission_memory_limit: 0,
            }),
            eviction: Some(super::Eviction {
                enabled: true,
                soft_limit: Some(0.80),
                replicas: Some(4),
                hard_limit: Some(0.85),
                check_interval: Some(Duration::from_millis(100)),
            }),
            lifetime: Some(super::Lifetime {
                enabled: false,
                on_ttl: Some(super::TTLMode::Refresh),
                ttl: Some(Duration::from_secs(24 * 3600)),
                replicas: Some(4),
                rate: Some(50),
                beta: Some(0.4),
                coefficient: Some(0.5),
                is_remove_on_ttl: Arc::new(AtomicBool::new(false)),
            }),
            traces: Some(super::Traces {
                enabled: false,
                service_name: None,
                service_version: None,
                exporter: None,
                endpoint: None,
                insecure: None,
                sampling_mode: None,
                sampling_rate: None,
                export_batch_size: None,
                export_batch_timeout: None,
                export_max_queue: None,
            }),
            metrics: Some(super::Metrics { enabled: true }),
            k8s: Some(super::K8S {
                probe: super::Probe {
                    timeout: Some(Duration::from_secs(5)),
                },
            }),
            rules: None,
            rules_raw: Some(HashMap::new()),
        },
    };

    // --- Rules ---
    let mut rules = HashMap::new();

    let key_query = vec![
        "user[id]".to_string(),
        "domain".to_string(),
        "language".to_string(),
        "picked".to_string(),
        "timezone".to_string(),
        "ns".to_string(),
    ];
    let key_headers = vec!["Accept-Encoding".to_string()];
    let value_headers_pd = vec![
        "Content-Type".to_string(),
        "Content-Encoding".to_string(),
        "Cache-Control".to_string(),
        "Vary".to_string(),
        "Strict-Transport-Security".to_string(),
        "X-Content-Digest".to_string(),
        "X-Error-Reason".to_string(),
    ];
    let value_headers_with_len = {
        let mut v = value_headers_pd.clone();
        v.insert(5, "Content-Length".to_string());
        v
    };

    // /api/v1/user (has lifetime refresh)
    rules.insert(
        "/api/v1/user".to_string(),
        super::Rule {
            path: Some("/api/v1/user".to_string()),
            path_bytes: Some(b"/api/v1/user".to_vec()),
            cache_key: super::RuleKey {
                query: Some(key_query.clone()),
                query_bytes: None,
                headers: Some(key_headers.clone()),
                headers_map: None,
            },
            cache_value: super::RuleValue {
                headers: Some(value_headers_pd.clone()),
                headers_map: None,
            },
            refresh: Some(super::LifetimeRule {
                enabled: true,
                ttl: Some(Duration::from_secs(60)),
                beta: Some(0.4),
                coefficient: Some(0.5),
            }),
        },
    );

    // /api/v1/client (no lifetime)
    rules.insert(
        "/api/v1/client".to_string(),
        super::Rule {
            path: Some("/api/v1/client".to_string()),
            path_bytes: Some(b"/api/v1/client".to_vec()),
            cache_key: super::RuleKey {
                query: Some(key_query.clone()),
                query_bytes: None,
                headers: Some(key_headers.clone()),
                headers_map: None,
            },
            cache_value: super::RuleValue {
                headers: Some(value_headers_with_len.clone()),
                headers_map: None,
            },
            refresh: None,
        },
    );

    // /api/v1/buyer
    rules.insert(
        "/api/v1/buyer".to_string(),
        super::Rule {
            path: Some("/api/v1/buyer".to_string()),
            path_bytes: Some(b"/api/v1/buyer".to_vec()),
            cache_key: super::RuleKey {
                query: Some(key_query.clone()),
                query_bytes: None,
                headers: Some(key_headers.clone()),
                headers_map: None,
            },
            cache_value: super::RuleValue {
                headers: Some(value_headers_with_len.clone()),
                headers_map: None,
            },
            refresh: None,
        },
    );

    // /api/v1/customer
    rules.insert(
        "/api/v1/customer".to_string(),
        super::Rule {
            path: Some("/api/v1/customer".to_string()),
            path_bytes: Some(b"/api/v1/customer".to_vec()),
            cache_key: super::RuleKey {
                query: Some(key_query.clone()),
                query_bytes: None,
                headers: Some(key_headers.clone()),
                headers_map: None,
            },
            cache_value: super::RuleValue {
                headers: Some(value_headers_with_len.clone()),
                headers_map: None,
            },
            refresh: None,
        },
    );

    cfg.cache.rules_raw = Some(rules);

    // --- Derive runtime fields ---
    if let Some(ref mut admission) = cfg.cache.admission {
        admission.is_enabled = Arc::new(AtomicBool::new(admission.enabled));
    }

    if let Some(ref mut storage) = cfg.cache.storage {
        storage.is_listing = storage.mode.as_deref() == Some("listing");
    }

    // Process rules_raw: query bytes and header maps, then convert to Arc<Rule>
    if let Some(ref mut rules_raw) = cfg.cache.rules_raw {
        let default_lifetime = cfg.cache.lifetime.as_ref().cloned();
        let mut processed_rules = HashMap::new();
        
        for (path, mut rule) in rules_raw.drain() {
            rule.path = Some(path.clone());
            rule.path_bytes = Some(path.as_bytes().to_vec());

            if rule.refresh.is_none() {
                if let Some(ref lifetime) = default_lifetime {
                    rule.refresh = Some(super::LifetimeRule {
                        enabled: lifetime.enabled,
                        ttl: lifetime.ttl,
                        beta: lifetime.beta,
                        coefficient: lifetime.coefficient,
                    });
                }
            }

            if let Some(ref queries) = rule.cache_key.query {
                rule.cache_key.query_bytes =
                    Some(queries.iter().map(|q| q.as_bytes().to_vec()).collect());
            }

            if let Some(ref headers) = rule.cache_key.headers {
                let mut headers_map = HashMap::new();
                for header in headers {
                    headers_map.insert(header.to_lowercase(), header.as_bytes().to_vec());
                }
                rule.cache_key.headers_map = Some(headers_map);
            }

            if let Some(ref headers) = rule.cache_value.headers {
                rule.cache_value.headers_map =
                    Some(headers.iter().cloned().collect::<HashSet<_>>());
            }
            
            // Wrap in Arc and store
            processed_rules.insert(path, Arc::new(rule));
        }
        
        cfg.cache.rules = Some(processed_rules);
        cfg.cache.rules_raw = None;
    }

    // Process upstream backend bytes
    if let Some(ref mut upstream) = cfg.cache.upstream {
        if let Some(ref mut backend) = upstream.backend {
            if let Some(ref id) = backend.id {
                backend.id_bytes = Some(id.as_bytes().to_vec());
            }
            if let Some(ref scheme) = backend.scheme {
                backend.scheme_bytes = Some(scheme.as_bytes().to_vec());
            }
            if let Some(ref host) = backend.host {
                backend.host_bytes = Some(host.as_bytes().to_vec());
            }
            if let Some(ref header) = backend.use_max_timeout_header {
                backend.use_max_timeout_header_bytes = Some(header.as_bytes().to_vec());
            }
            if let Some(ref health) = backend.healthcheck {
                backend.healthcheck_bytes = Some(health.as_bytes().to_vec());
            }
        }
    }

    // Compute memory limits from eviction + storage
    if let (Some(ev), Some(storage)) = (cfg.cache.eviction.as_ref(), cfg.cache.storage.as_mut()) {
        let soft = ev.soft_limit.unwrap_or(0.8);
        let hard = ev.hard_limit.unwrap_or(0.99);
        let size = storage.size;
        storage.soft_memory_limit = (size as f64 * soft) as i64;
        storage.hard_memory_limit = (size as f64 * hard) as i64;
        storage.admission_memory_limit = storage.soft_memory_limit - (100 << 20);
    }

    // Lifetime on_ttl flag
    if let Some(ref lifetime) = cfg.cache.lifetime {
        if let Some(on_ttl) = lifetime.on_ttl {
            lifetime
                .is_remove_on_ttl
                .store(on_ttl == super::TTLMode::Remove, Ordering::Relaxed);
        }
    }

    cfg
}
