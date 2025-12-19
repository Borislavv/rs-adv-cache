// Cache storage implementation with worker orchestration.

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::config::{Config, ConfigTrait};
use crate::governor::Governor;
use crate::model::Entry;
use crate::upstream::Upstream;


// Constants
const COMP_STORAGE: &str = "storage";
const COMP_DUMP: &str = "dump";
pub const SVC_EVICTOR: &str = "soft-eviction";
pub const SVC_LIFETIME_MANAGER: &str = "wrk-lifetime-manager";

/// Trait for cache storage backends.
#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    /// Retrieves an entry from storage, returning the entry and a hit flag.
    fn get(&self, entry: &Entry) -> (Option<Entry>, bool);

    /// Retrieves an entry by its numeric key.
    fn get_by_key(&self, key: u64) -> (Option<Entry>, bool);

    /// Stores an entry in storage, returning whether it was persisted.
    fn set(&self, entry: Entry) -> bool;

    /// Walks through all shards, calling the provided function for each shard.
    fn walk_shards(&self,ctx: CancellationToken,f: Box<dyn FnMut(u64, &crate::db::storage::Shard<Entry>) + Send + Sync>);

    /// Removes an entry from storage, returning freed bytes and a hit flag.
    fn remove(&self, entry: &Entry) -> (i64, bool);

    /// Returns storage statistics: (bytes, entry_count).
    fn stat(&self) -> (i64, i64);

    /// Clears all entries from storage.
    fn clear(&self);

    /// Gracefully closes storage.
    async fn close(&self) -> Result<()> {
        Ok(())
    }
}

/// Main storage database that wraps LRU storage and supervises worker groups.
pub struct DB {
    storage: Arc<crate::db::storage::Storage>,
    cfg: Config,
    shutdown_token: CancellationToken,
    governor: Arc<dyn Governor>,
    persistence: Arc<dyn Dumper>,
}

/// Trait for persistence operations.
pub use crate::db::persistance::Dumper;

impl DB {
    /// Constructs the storage, wires workers and starts the governor.
    /// All worker transitions are managed through the governor.
    pub fn new(
        ctx: CancellationToken,
        cfg: Config,
        gov: Arc<dyn Governor>,
        up: Arc<dyn Upstream>,
    ) -> Result<Arc<Self>> {
        // Core storage
        let sharded_map = Arc::new(crate::db::storage::Map::new(ctx.clone(), cfg.clone()));
        let storage = crate::db::storage::Storage::new(
            ctx.clone(),
            cfg.clone(),
            up.clone(),
            sharded_map,
        )
        .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Force groups with initial config (interval/enable).
        // Eviction worker
        let eviction_freq_cfg = crate::workers::CallFreq::new(
            0,
            cfg.eviction()
                .and_then(|e| e.check_interval)
                .unwrap_or(Duration::from_millis(100)),
        );
        let eviction_cfg = crate::workers::WorkerConfig::new(
            cfg.eviction().map(|e| e.enabled).unwrap_or(false),
            Arc::new(eviction_freq_cfg) as Arc<dyn crate::governor::Freq>,
            cfg.eviction().and_then(|e| e.replicas).unwrap_or(32),
        );
        let eviction = crate::workers::evictor::Evictor::new(
            ctx.clone(),
            SVC_EVICTOR.to_string(),
            Arc::new(eviction_cfg) as Arc<dyn crate::governor::Config>,
            storage.clone(),
        )?;

        // Lifetime manager worker
        let refresh_freq_cfg = crate::workers::CallFreq::new(
            cfg.lifetime().and_then(|l| l.rate).unwrap_or(1000) as usize,
            Duration::ZERO,
        );
        let refresh_cfg = crate::workers::WorkerConfig::new(
            cfg.lifetime().map(|l| l.enabled).unwrap_or(false),
            Arc::new(refresh_freq_cfg) as Arc<dyn crate::governor::Freq>,
            cfg.lifetime().and_then(|l| l.replicas).unwrap_or(32),
        );
        let refresh = crate::workers::lifetimer::LifetimeManager::new(
            ctx.clone(),
            SVC_LIFETIME_MANAGER.to_string(),
            Arc::new(refresh_cfg) as Arc<dyn crate::governor::Config>,
            cfg.clone(),
            storage.clone(),
        )
        .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Register services before starting governor to prevent race conditions.
        struct ServiceWrapper<T: 'static>(Arc<T>)
        where
            Arc<T>: crate::governor::Service;

        impl<T: 'static> crate::governor::Service for ServiceWrapper<T>
        where
            Arc<T>: crate::governor::Service,
        {
            fn name(&self) -> &str {
                self.0.name()
            }

            fn cfg(&self) -> Arc<dyn crate::governor::Config> {
                self.0.cfg()
            }

            fn replicas(&self) -> usize {
                self.0.replicas()
            }

            fn serve(&self, t: Arc<dyn crate::governor::Transport>) {
                self.0.serve(t)
            }

            fn transport(&self) -> Arc<dyn crate::governor::Transport> {
                self.0.transport()
            }
        }

        fn to_dyn_service<T>(s: Arc<T>) -> Arc<dyn crate::governor::Service>
        where
            T: 'static,
            Arc<T>: crate::governor::Service,
        {
            Arc::new(ServiceWrapper(s))
        }

        gov.register(SVC_EVICTOR.to_string(), to_dyn_service(eviction.clone()));
        gov.register(
            SVC_LIFETIME_MANAGER.to_string(),
            to_dyn_service(refresh.clone()),
        );
        // Starting workers
        let _ = gov.start(SVC_EVICTOR);
        let _ = gov.start(SVC_LIFETIME_MANAGER);

        // Enabled/disable workers
        if cfg.eviction().map(|e| e.enabled).unwrap_or(false) {
            let _ = gov.on(SVC_EVICTOR);
        } else {
            info!(name = SVC_EVICTOR, event = "on/off", "disabled");
        }
        if cfg.lifetime().map(|l| l.enabled).unwrap_or(false) {
            let _ = gov.on(SVC_LIFETIME_MANAGER);
        } else {
            info!(name = SVC_LIFETIME_MANAGER, event = "on/off", "disabled");
        }

        // Init. of the storage itself
        let db = Arc::new(Self {
            shutdown_token: ctx,
            cfg: cfg.clone(),
            governor: gov,
            storage: storage.clone(),
            persistence: new_dump(cfg, storage.clone())?,
        });

        Ok(db.run())
    }

    /// Runs initialization (load dump or mocks if enabled).
    fn run(self: Arc<Self>) -> Arc<Self> {
        if self.cfg.is_enabled() {
            if self
                .cfg
                .data()
                .and_then(|d| d.dump.as_ref())
                .map(|d| d.enabled)
                .unwrap_or(false)
            {
                // Load dump asynchronously
                let persistence = self.persistence.clone();
                let token = self.shutdown_token.clone();
                tokio::task::spawn(async move {
                    if let Err(e) = persistence.load(token).await {
                        error!(
                            component = COMP_DUMP,
                            event = "load_failed",
                            error = %e,
                            "error loading cache dump"
                        );
                    }
                });
            } else if self
                .cfg
                .data()
                .and_then(|d| d.mock.as_ref())
                .map(|m| m.enabled)
                .unwrap_or(false)
            {
                let length = self
                    .cfg
                    .data()
                    .and_then(|d| d.mock.as_ref())
                    .and_then(|m| m.length)
                    .unwrap_or(1000000);
                load_mocks(
                    self.shutdown_token.clone(),
                    self.cfg.clone(),
                    self.clone(),
                    length,
                );
            }
        }
        self
    }
}

#[async_trait::async_trait]
impl Storage for DB {
    fn get(&self, entry: &Entry) -> (Option<Entry>, bool) {
        self.storage.get(entry)
    }

    fn get_by_key(&self, key: u64) -> (Option<Entry>, bool) {
        let entry = self.storage.get_by_key(key);
        let hit = entry.is_some();
        (entry, hit)
    }

    fn set(&self, entry: Entry) -> bool {
        self.storage.set(entry)
    }

    fn walk_shards(
        &self,
        ctx: CancellationToken,
        f: Box<dyn FnMut(u64, &crate::db::storage::Shard<Entry>) + Send + Sync>,
    ) {
        self.storage.walk_shards(ctx, f);
    }

    fn remove(&self, entry: &Entry) -> (i64, bool) {
        self.storage.remove(entry)
    }

    fn stat(&self) -> (i64, i64) {
        self.storage.stat()
    }

    fn clear(&self) {
        self.storage.clear();
    }

    async fn close(&self) -> Result<()> {
        let stop_ctx = CancellationToken::new();

        if self.cfg.is_enabled()
            && self
                .cfg
                .data()
                .and_then(|d| d.dump.as_ref())
                .map(|d| d.enabled)
                .unwrap_or(false)
        {
            match timeout(
                Duration::from_secs(60),
                self.persistence.dump(stop_ctx.clone()),
            )
            .await
            {
                Ok(result) => {
                    if let Err(e) = result {
                        error!(
                            component = COMP_DUMP,
                            event = "store_failed",
                            error = %e,
                            "failed to store cache dump"
                        );
                    }
                }
                Err(_) => {
                    error!(
                        component = COMP_DUMP,
                        event = "store_timeout",
                        "dump operation timed out"
                    );
                }
            }
        }

        if let Err(e) = self.storage.close().await {
            error!(
                component = COMP_STORAGE,
                event = "close_failed",
                error = %e,
                "storage close failed"
            );
        }

        self.governor.stop();
        self.shutdown_token.cancel();
        Ok(())
    }
}

impl Drop for DB {
    fn drop(&mut self) {
        // Ensure worker groups receive stop signal on shutdown.
        self.governor.stop();
    }
}

/// Creates a new dumper instance.
fn new_dump(
    cfg: Config,
    storage: Arc<crate::db::storage::Storage>,
) -> Result<Arc<dyn Dumper>> {
    Ok(Arc::new(crate::db::persistance::DumperImpl::new(
        cfg,
        storage.clone() as Arc<dyn Storage>,
    )?))
}

/// Loads mock data into storage.
pub fn load_mocks(ctx: CancellationToken, cfg: Config, storage: Arc<dyn Storage>, num: usize) {
    load_mocks_with(ctx, cfg, storage, num, false);
}

/// Loads mock data with optional brotli compression.
fn load_mocks_with(
    ctx: CancellationToken,
    cfg: Config,
    storage: Arc<dyn Storage>,
    num: usize,
    brotli: bool,
) {
    tokio::task::spawn(async move {
        info!(
            component = "mocks",
            event = "loading_start",
            "start loading mock data"
        );

        let path = b"/api/v1/user";
        for i in 0..num {
            if ctx.is_cancelled() {
                break;
            }
            let entry = get_single_mock_with(i, path, cfg.clone(), brotli);
            storage.set(entry);
        }

        info!(
            component = "mocks",
            event = "loading_finish",
            "finished loading mock data"
        );
    });
}

/// Gets a single mock entry.
fn get_single_mock_with(i: usize, path: &[u8], cfg: Config, brotli: bool) -> Entry {
    use crate::model::{match_cache_rule, Response};
    use std::sync::Arc;

    // Try to match a rule for the path, or create a default rule
    let rule = match match_cache_rule(&cfg, path) {
        Ok(r) => r, // Already Arc<Rule>, just use it
        Err(_) => {
            // Create a default rule if no match found
            Arc::new(crate::config::Rule {
                path: Some(String::from_utf8_lossy(path).to_string()),
                path_bytes: Some(path.to_vec()),
                cache_key: crate::config::RuleKey {
                    query: None,
                    query_bytes: None,
                    headers: None,
                    headers_map: None,
                },
                cache_value: crate::config::RuleValue {
                    headers: None,
                    headers_map: None,
                },
                refresh: None,
            })
        }
    };

    // Create empty queries and headers for mock
    let queries: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let headers: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();

    // Create entry
    let entry = Entry::new(rule, &queries, &headers);

    // Create mock JSON response body
    let mock_json = format!(
        r#"{{
      "response": {{
        "status": "ok",
        "payload": {{
          "id": "item-10",
          "context": {{
            "label": "Mock Label [{}]",
            "tags": [
              "example",
              "tag-placeholder"
            ]
          }},
          "content": {{
            "header": "Header for item [{}]",
            "summary": "This is a mock summary for. Placeholder inserted.",
            "details": {{
              "info": "Extra information with repeated placeholder.",
              "active": true,
              "score": 10,
            }},
            "assets": {{
              "images": [],
              "videos": null
            }}
          }}
        }},
        "meta": {{
          "generatedAt": "2025-01-01T00:00:00Z",
          "mockSource": "advCache-Mock-v2"
        }}
      }}
    }}"#,
        i, i
    );

    let body = if brotli {
        // Compress with brotli if requested
        use brotli::enc::BrotliEncoderParams;
        let mut compressed = Vec::new();
        if brotli::BrotliCompress(
            &mut mock_json.as_bytes(),
            &mut compressed,
            &BrotliEncoderParams::default(),
        )
        .is_err()
        {
            // Fallback to uncompressed if compression fails
            compressed = mock_json.as_bytes().to_vec();
        }
        compressed
    } else {
        mock_json.as_bytes().to_vec()
    };

    // Create mock response
    let response = Response {
        status: 200,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body,
    };

    // Set payload
    entry.set_payload(&queries, &headers, &response);

    // Set updated timestamp
        entry.updated_at_ref().store(
        crate::time::unix_nano(),
        std::sync::atomic::Ordering::Relaxed,
    );

    entry
}
