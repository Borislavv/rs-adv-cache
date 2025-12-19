// Configuration loading and management.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const PROD: &str = "prod";
#[allow(dead_code)]
pub const DEV: &str = "dev";
#[allow(dead_code)]
pub const DEBUG: &str = "debug";
#[allow(dead_code)]
pub const TEST: &str = "test";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SamplingMode {
    Off,    // never record
    Always, // record all
    Ratio,  // ParentBased(TraceIDRatioBased)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Cache {
    #[serde(rename = "cache")]
    pub cache: CacheBox,
}

impl Clone for Cache {
    fn clone(&self) -> Self {
        // Clone is expensive but needed for some tests
        // Rules are shared via Arc, so cloning is safe
        Self {
            cache: CacheBox {
                env: self.cache.env.clone(),
                enabled: self.cache.enabled,
                atomic_enabled: Arc::new(AtomicBool::new(self.cache.atomic_enabled.load(Ordering::Relaxed))),
                logs: self.cache.logs.clone(),
                runtime: self.cache.runtime.clone(),
                api: self.cache.api.clone(),
                upstream: self.cache.upstream.clone(),
                data: self.cache.data.clone(),
                storage: self.cache.storage.clone(),
                compression: self.cache.compression.clone(),
                eviction: self.cache.eviction.clone(),
                admission: self.cache.admission.clone(),
                traces: self.cache.traces.clone(),
                lifetime: self.cache.lifetime.clone(),
                metrics: self.cache.metrics.clone(),
                k8s: self.cache.k8s.clone(),
                rules: self.cache.rules.as_ref().map(|rules| {
                    rules.iter().map(|(k, v)| (k.clone(), Arc::clone(v))).collect()
                }),
                rules_raw: None, // rules_raw is only used during deserialization
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CacheBox {
    pub env: String,
    pub enabled: bool,
    #[serde(skip)]
    pub atomic_enabled: Arc<AtomicBool>,
    pub logs: Option<Logs>,
    pub runtime: Option<Runtime>,
    pub api: Option<Api>,
    pub upstream: Option<Upstream>,
    pub data: Option<Data>,
    pub storage: Option<Storage>,
    pub compression: Option<Compression>,
    pub eviction: Option<Eviction>,
    pub admission: Option<Admission>,
    pub traces: Option<Traces>,
    pub lifetime: Option<Lifetime>,
    pub metrics: Option<Metrics>,
    pub k8s: Option<K8S>,
    #[serde(skip)]
    pub rules: Option<HashMap<String, Arc<Rule>>>,
    #[serde(rename = "rules")]
    rules_raw: Option<HashMap<String, Rule>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Admission {
    pub enabled: bool,
    #[serde(skip)]
    pub is_enabled: Arc<AtomicBool>,
    pub capacity: Option<usize>,
    pub shards: Option<usize>,
    #[serde(rename = "min_table_len_per_shard")]
    pub min_table_len_per_shard: Option<usize>,
    #[serde(rename = "sample_multiplier")]
    pub sample_multiplier: Option<usize>,
    #[serde(rename = "door_bits_per_counter")]
    pub door_bits_per_counter: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Traces {
    pub enabled: bool,
    #[serde(rename = "service_name")]
    pub service_name: Option<String>,
    #[serde(rename = "service_version")]
    pub service_version: Option<String>,
    pub exporter: Option<String>,
    pub endpoint: Option<String>,
    pub insecure: Option<bool>,
    #[serde(rename = "sampling_mode")]
    pub sampling_mode: Option<SamplingMode>,
    #[serde(rename = "sampling_rate")]
    pub sampling_rate: Option<f64>,
    #[serde(rename = "export_batch_size")]
    pub export_batch_size: Option<usize>,
    #[serde(rename = "export_batch_timeout", with = "humantime_serde")]
    pub export_batch_timeout: Option<Duration>,
    #[serde(rename = "export_max_queue")]
    pub export_max_queue: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Api {
    pub name: Option<String>,
    pub port: Option<String>,
}

impl Clone for Api {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            port: self.port.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Runtime {
    pub num_cpus: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Probe {
    #[serde(with = "humantime_serde")]
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct K8S {
    pub probe: Probe,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Metrics {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Logs {
    pub level: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Upstream {
    pub policy: Option<String>,
    pub cluster: Option<Cluster>,
    pub backend: Option<Backend>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Cluster {
    pub backends: Option<Vec<Backend>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Backend {
    pub id: Option<String>,
    #[serde(skip)]
    pub id_bytes: Option<Vec<u8>>,
    pub enabled: bool,
    pub policy: Option<String>,
    pub scheme: Option<String>,
    #[serde(skip)]
    pub scheme_bytes: Option<Vec<u8>>,
    pub host: Option<String>,
    #[serde(skip)]
    pub host_bytes: Option<Vec<u8>>,
    pub rate: Option<usize>,
    pub concurrency: Option<usize>,
    #[serde(with = "humantime_serde")]
    pub timeout: Option<Duration>,
    #[serde(rename = "max_timeout", with = "humantime_serde")]
    pub max_timeout: Option<Duration>,
    #[serde(rename = "use_max_timeout_header")]
    pub use_max_timeout_header: Option<String>,
    #[serde(skip)]
    pub use_max_timeout_header_bytes: Option<Vec<u8>>,
    pub healthcheck: Option<String>,
    #[serde(skip)]
    pub healthcheck_bytes: Option<Vec<u8>>,
    pub addr: Option<String>,
    #[serde(rename = "health_path")]
    pub health_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dump {
    pub enabled: bool,
    #[serde(rename = "dump_dir")]
    pub dir: Option<String>,
    #[serde(rename = "dump_name")]
    pub name: Option<String>,
    #[serde(rename = "max_versions")]
    pub max_versions: Option<usize>,
    pub gzip: bool,
    #[serde(rename = "crc32_control_sum")]
    pub crc32_control: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Mock {
    pub enabled: bool,
    pub length: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Data {
    pub dump: Option<Dump>,
    pub mock: Option<Mock>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Eviction {
    pub enabled: bool,
    #[serde(rename = "soft_limit")]
    pub soft_limit: Option<f64>,
    #[serde(rename = "hard_limit")]
    pub hard_limit: Option<f64>,
    pub replicas: Option<usize>,
    #[serde(rename = "check_interval", with = "humantime_serde")]
    pub check_interval: Option<Duration>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Storage {
    pub mode: Option<String>,
    #[serde(skip)]
    pub is_listing: bool,
    pub size: i64,
    #[serde(skip)]
    pub soft_memory_limit: i64,
    #[serde(skip)]
    pub hard_memory_limit: i64,
    #[serde(skip)]
    pub admission_memory_limit: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Compression {
    pub enabled: bool,
    pub level: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TTLMode {
    Remove,
    Refresh,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Lifetime {
    pub enabled: bool,
    #[serde(rename = "on_ttl")]
    pub on_ttl: Option<TTLMode>,
    #[serde(rename = "ttl", with = "humantime_serde")]
    pub ttl: Option<Duration>,
    pub replicas: Option<usize>,
    pub rate: Option<usize>,
    pub beta: Option<f64>,
    pub coefficient: Option<f64>,
    #[serde(skip)]
    pub is_remove_on_ttl: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LifetimeRule {
    pub enabled: bool,
    #[serde(rename = "ttl", with = "humantime_serde")]
    pub ttl: Option<Duration>,
    pub beta: Option<f64>,
    pub coefficient: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rule {
    #[serde(skip)]
    pub path: Option<String>,
    #[serde(skip)]
    pub path_bytes: Option<Vec<u8>>,
    #[serde(rename = "cache_key")]
    pub cache_key: RuleKey,
    #[serde(rename = "cache_value")]
    pub cache_value: RuleValue,
    pub refresh: Option<LifetimeRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuleKey {
    pub query: Option<Vec<String>>,
    #[serde(skip)]
    pub query_bytes: Option<Vec<Vec<u8>>>,
    pub headers: Option<Vec<String>>,
    #[serde(skip)]
    pub headers_map: Option<HashMap<String, Vec<u8>>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuleValue {
    pub headers: Option<Vec<String>>,
    #[serde(skip)]
    pub headers_map: Option<std::collections::HashSet<String>>,
}

// Config trait
pub trait ConfigTrait {
    fn logs(&self) -> Option<&Logs>;
    fn is_prod(&self) -> bool;
    #[allow(dead_code)]
    fn is_debug(&self) -> bool;
    #[allow(dead_code)]
    fn is_dev(&self) -> bool;
    #[allow(dead_code)]
    fn is_test(&self) -> bool;
    fn is_enabled(&self) -> bool;
    fn set_enabled(&self, v: bool);
    fn runtime(&self) -> &Runtime;
    fn api(&self) -> Option<&Api>;
    fn upstream(&self) -> Option<&Upstream>;
    fn data(&self) -> Option<&Data>;
    fn lifetime(&self) -> Option<&Lifetime>;
    fn eviction(&self) -> Option<&Eviction>;
    fn admission(&self) -> Option<&Admission>;
    fn traces(&self) -> Option<&Traces>;
    fn storage(&self) -> &Storage;
    fn compression(&self) -> Option<&Compression>;
    fn k8s(&self) -> Option<&K8S>;
    fn rule(&self, path: &str) -> Option<Arc<Rule>>;
}

// Config type alias for convenience
pub type Config = Cache;

impl ConfigTrait for Config {
    fn logs(&self) -> Option<&Logs> {
        self.cache.logs.as_ref()
    }

    fn is_prod(&self) -> bool {
        self.cache.env == PROD
    }

    fn is_debug(&self) -> bool {
        self.cache.env == DEBUG
    }

    fn is_dev(&self) -> bool {
        self.cache.env == DEV
    }

    fn is_test(&self) -> bool {
        self.cache.env == TEST
    }

    fn is_enabled(&self) -> bool {
        self.cache.atomic_enabled.load(Ordering::Relaxed)
    }

    fn set_enabled(&self, v: bool) {
        self.cache.atomic_enabled.store(v, Ordering::Relaxed);
    }

    fn runtime(&self) -> &Runtime {
        self.cache
            .runtime
            .as_ref()
            .unwrap_or(&Runtime { num_cpus: 0 })
    }

    fn api(&self) -> Option<&Api> {
        self.cache.api.as_ref()
    }

    fn upstream(&self) -> Option<&Upstream> {
        self.cache.upstream.as_ref()
    }

    fn data(&self) -> Option<&Data> {
        self.cache.data.as_ref()
    }

    fn lifetime(&self) -> Option<&Lifetime> {
        self.cache.lifetime.as_ref()
    }

    fn eviction(&self) -> Option<&Eviction> {
        self.cache.eviction.as_ref()
    }

    fn admission(&self) -> Option<&Admission> {
        self.cache.admission.as_ref()
    }

    fn traces(&self) -> Option<&Traces> {
        self.cache.traces.as_ref()
    }

    fn storage(&self) -> &Storage {
        self.cache
            .storage
            .as_ref()
            .expect("storage config is required")
    }

    fn compression(&self) -> Option<&Compression> {
        self.cache.compression.as_ref()
    }

    fn k8s(&self) -> Option<&K8S> {
        self.cache.k8s.as_ref()
    }

    fn rule(&self, path: &str) -> Option<Arc<Rule>> {
        self.cache.rules.as_ref()?.get(path).map(Arc::clone)
    }
}

impl Config {
    /// Loads configuration from a YAML file.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Resolve absolute path
        let abs_path = path
            .canonicalize()
            .with_context(|| format!("failed to resolve absolute config filepath: {:?}", path))?;

        // Read file
        let data = std::fs::read_to_string(&abs_path)
            .with_context(|| format!("read config yaml file {:?}", abs_path))?;

        // Parse YAML
        let mut cfg: Cache = serde_yaml::from_str(&data)
            .with_context(|| format!("unmarshal yaml from {:?}", abs_path))?;

        // Initialize atomic fields
        cfg.cache.atomic_enabled = Arc::new(AtomicBool::new(cfg.cache.enabled));

        if let Some(ref mut admission) = cfg.cache.admission {
            admission.is_enabled = Arc::new(AtomicBool::new(admission.enabled));
        }

        // Process storage mode
        const LISTING_MODE: &str = "listing";
        if let Some(ref mut storage) = cfg.cache.storage {
            storage.is_listing = storage.mode.as_deref() == Some(LISTING_MODE);
        }

        if let Some(ref mut rules_raw) = cfg.cache.rules_raw {
            let default_lifetime = cfg.cache.lifetime.as_ref().cloned();
            let mut processed_rules = HashMap::new();
            for (rule_path, mut rule) in rules_raw.drain() {
                rule.path = Some(rule_path.clone());
                rule.path_bytes = Some(rule_path.as_bytes().to_vec());

                if rule.refresh.is_none() {
                    if let Some(ref lifetime) = default_lifetime {
                        rule.refresh = Some(LifetimeRule {
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

                // Process value headers map
                if let Some(ref headers) = rule.cache_value.headers {
                    rule.cache_value.headers_map = Some(headers.iter().cloned().collect());
                }
                
                // Wrap in Arc and store
                processed_rules.insert(rule_path, Arc::new(rule));
            }
            cfg.cache.rules = Some(processed_rules);
            cfg.cache.rules_raw = None; // Clear raw rules after processing
        }

        // Process upstream backends
        if let Some(ref mut upstream) = cfg.cache.upstream {
            if let Some(ref mut cluster) = upstream.cluster {
                if let Some(ref mut backends) = cluster.backends {
                    for backend in backends.iter_mut() {
                        Self::process_backend(backend);
                    }
                }
            } else if let Some(ref mut backend) = upstream.backend {
                Self::process_backend(backend);
            } else {
                anyhow::bail!("no backend configured");
            }
        }

        let (soft_limit, hard_limit, size) = if let Some(eviction) = cfg.eviction() {
            let soft = eviction.soft_limit.unwrap_or(0.8);
            let hard = eviction.hard_limit.unwrap_or(0.99);
            let storage_size = cfg.cache.storage.as_ref().map(|s| s.size).unwrap_or(0);
            (soft, hard, storage_size)
        } else {
            (0.8, 0.99, 0)
        };

        if let Some(ref mut storage) = cfg.cache.storage {
            storage.soft_memory_limit = (size as f64 * soft_limit) as i64;
            storage.hard_memory_limit = (size as f64 * hard_limit) as i64;
            storage.admission_memory_limit = storage.soft_memory_limit - (100 << 20);
            // soft - 100mb
        }

        // Process lifetime TTL mode
        if let Some(ref mut lifetime) = cfg.cache.lifetime {
            if let Some(on_ttl) = lifetime.on_ttl {
                lifetime.is_remove_on_ttl = Arc::new(AtomicBool::new(on_ttl == TTLMode::Remove));
            } else {
                anyhow::bail!("invalid lifetime.OnTTLMode configured");
            }
        }

        Ok(cfg)
    }

    fn process_backend(backend: &mut Backend) {
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
        if let Some(ref healthcheck) = backend.healthcheck {
            backend.healthcheck_bytes = Some(healthcheck.as_bytes().to_vec());
        }
    }
}

// Test config is always available for integration tests
mod test_config;
#[allow(dead_code)]
pub use test_config::new_test_config;
