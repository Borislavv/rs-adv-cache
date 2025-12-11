use super::{CacheBox, Config};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

/// Creates a new test configuration.
pub fn new_test_config() -> Config {
    Config {
        cache: CacheBox {
            env: super::TEST.to_string(),
            enabled: true,
            atomic_enabled: Arc::new(AtomicBool::new(true)),
            logs: Some(super::Logs {
                level: Some("debug".to_string()),
            }),
            runtime: Some(super::Runtime {
                num_cpus: 12,
            }),
            api: Some(super::Api {
                name: Some("adv_cache:8091".to_string()),
                port: Some("8091".to_string()),
                net_listener: None,
            }),
            upstream: Some(super::Upstream {
                policy: None,
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
                    healthcheck: Some("/healthcheck".to_string()),
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
                level: Some(1), // BestSpeed equivalent
            }),
            storage: Some(super::Storage {
                mode: Some("listing".to_string()),
                is_listing: true,
                size: 1036870900,
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
                service_tenant: None,
                exporter: None,
                endpoint: None,
                insecure: None,
                sampling_mode: None,
                sampling_rate: None,
                export_batch_size: None,
                export_batch_timeout: None,
                export_max_queue: None,
            }),
            metrics: None,
            k8s: None,
            rules: Some(std::collections::HashMap::new()),
        },
    }
}
