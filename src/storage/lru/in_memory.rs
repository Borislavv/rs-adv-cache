// Package lru provides in-memory LRU storage implementation.

use anyhow::Result;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::config::{Config, ConfigTrait};
use crate::model::Entry;
use crate::storage::lfu::Admission;
use crate::storage::map::Map;
use crate::upstream::Upstream;

use super::logger;

const SHARDS_SAMPLE: i64 = 2;
const KEYS_SAMPLE: i64 = 8;
const SPINS_BACKOFF: i64 = 32;

/// In-memory LRU storage.
pub struct InMemoryStorage {
    cfg: Config,
    upstream: Arc<dyn Upstream>,
    admitter: Arc<dyn Admission>,
    soft_memory_limit: i64,
    hard_memory_limit: i64,
    admission_memory_limit: i64,
    smap: Arc<Map<Entry>>,
}

impl InMemoryStorage {
    /// Creates a new in-memory storage.
    pub fn new(
        shutdown_token: CancellationToken,
        cfg: Config,
        upstream: Arc<dyn Upstream>,
        sharded_map: Arc<Map<Entry>>,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let admitter_box = crate::storage::lfu::new_admission(cfg.admission());
        let admitter = Arc::from(admitter_box);

        let storage = Arc::new(Self {
            cfg: cfg.clone(),
            upstream,
            admitter,
            soft_memory_limit: cfg.storage().soft_memory_limit,
            hard_memory_limit: cfg.storage().hard_memory_limit,
            admission_memory_limit: cfg.storage().admission_memory_limit,
            smap: sharded_map,
        });

        // Start logger
        let storage_clone = storage.clone();
        let cfg_arc = Arc::new(tokio::sync::RwLock::new(cfg));
        let mem_fn: Arc<dyn Fn() -> i64 + Send + Sync> = Arc::new({
            let smap = storage_clone.smap.clone();
            move || smap.mem()
        });
        let len_fn: Arc<dyn Fn() -> i64 + Send + Sync> = Arc::new({
            let smap = storage_clone.smap.clone();
            move || smap.len()
        });
        tokio::task::spawn(async move {
            logger::logger(
                shutdown_token,
                cfg_arc,
                storage_clone.soft_memory_limit,
                storage_clone.hard_memory_limit,
                mem_fn,
                len_fn,
            ).await;
        });

        Ok(storage)
    }

    /// Gets an entry by key.
    pub fn get_by_key(&self, key: u64) -> Option<Entry> {
        self.smap.get(key)
    }

    /// Gets an entry matching the request.
    pub fn get(&self, req: &Entry) -> (Option<Entry>, bool) {
        if let Some(mut ptr) = self.smap.get(req.key()) {
            if ptr.is_the_same_fingerprint(req) {
                self.touch(&mut ptr);
                return (Some(ptr), true);
            }
        }
        (None, false)
    }

    /// Sets or updates an entry.
    pub fn set(&self, new: Entry) -> bool {
        let key = new.key();
        self.admitter.record(key);

        if let Some(mut old) = self.smap.get(key) {
            if old.is_the_same_fingerprint(&new) {
                if old.is_the_same_payload(&new) {
                    self.touch(&mut old);
                    // Update the entry in the map
                    self.smap.set(key, old);
                } else {
                    let mut new_mut = new;
                    self.update(&mut old, &mut new_mut);
                    // Update the entry in the map
                    self.smap.set(key, old);
                }
                return true;
            }
            // Hash collision; will be rewritten by new insertion
        }

        if self.admission_memory_limit_overcome() {
            if let Some((_sh, victim)) = self.smap.pick_victim(SHARDS_SAMPLE, KEYS_SAMPLE) {
                if !self.admitter.allow(key, victim.key()) {
                    logger::ADMISSION_NOT_ALLOWED.fetch_add(1, Ordering::Relaxed);
                    return false;
                } else {
                    logger::ADMISSION_ALLOWED.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        if self.hard_memory_limit_overcome() {
            let (freed_bytes, items) = self.hard_evict_until_within_limit();
            if freed_bytes > 0 || items > 0 {
                logger::EVICTED_HARD_LIMIT_ITEMS.fetch_add(items, Ordering::Relaxed);
                logger::EVICTED_HARD_LIMIT_BYTES.fetch_add(freed_bytes, Ordering::Relaxed);
            }
        }

        self.smap.set(key, new);
        true
    }

    /// Touches an existing entry (updates access time).
    fn touch(&self, existing: &mut Entry) {
        existing.touch();
        // Move to front in LRU list
        self.smap.touch(existing.key());
        // Lifetime-on-access: enqueue
        if existing.is_expired(&self.cfg) && existing.try_mark_refresh_queued() {
            if !self.smap.enqueue_expired(existing.key()) {
                existing.clear_refresh_queued();
            }
        }
    }

    /// Updates an existing entry with new payload.
    fn update(&self, existing: &mut Entry, in_entry: &mut Entry) {
        let bytes_delta = existing.swap_payloads(in_entry);
        self.smap.add_mem(existing.key(), bytes_delta);
        existing.touch();
        existing.touch_refreshed_at();
        existing.clear_refresh_queued();
        self.smap.touch(existing.key());
    }

    /// Handles TTL expiration (internal implementation).
    async fn on_ttl_internal(&self, entry: &mut Entry) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.cfg.lifetime()
            .map(|l| l.is_remove_on_ttl.load(Ordering::Relaxed))
            .unwrap_or(false)
        {
            self.remove(entry);
            Ok(())
        } else {
            self.upstream.refresh(entry).await.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { 
                Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("{}", e)))
            })
        }
    }

    /// Gets the number of entries.
    pub fn len(&self) -> i64 {
        self.smap.len()
    }

    /// Gets the total memory usage in bytes.
    pub fn mem(&self) -> i64 {
        self.smap.mem()
    }

    /// Gets statistics (bytes, length).
    pub fn stat(&self) -> (i64, i64) {
        (self.smap.mem(), self.smap.len())
    }

    /// Clears all entries.
    pub fn clear(&self) {
        self.smap.clear();
    }

    /// Removes an entry.
    pub fn remove(&self, entry: &Entry) -> (i64, bool) {
        self.smap.remove(entry.key())
    }

    /// Evicts entries until within soft limit.
    pub fn soft_evict_until_within_limit(&self, backoff: i64) -> (i64, i64) {
        self.smap.evict_until_within_limit(self.soft_memory_limit, backoff)
    }

    /// Evicts entries until within hard limit.
    fn hard_evict_until_within_limit(&self) -> (i64, i64) {
        self.smap.evict_until_within_limit(self.hard_memory_limit, SPINS_BACKOFF)
    }

    /// Peeks at an expired entry with TTL.
    pub fn peek_expired_ttl(&self) -> Option<Entry> {
        self.smap.peek_expired_ttl()
    }

    /// Checks if soft memory limit is exceeded.
    pub fn soft_memory_limit_overcome(&self) -> bool {
        self.smap.len() > 0 && self.smap.mem() - self.soft_memory_limit > 0
    }

    /// Checks if hard memory limit is exceeded.
    fn hard_memory_limit_overcome(&self) -> bool {
        self.smap.len() > 0 && self.smap.mem() - self.hard_memory_limit > 0
    }

    /// Checks if admission memory limit is exceeded.
    fn admission_memory_limit_overcome(&self) -> bool {
        self.cfg.admission()
            .map(|a| a.is_enabled.load(Ordering::Relaxed))
            .unwrap_or(false)
            && self.smap.len() > 0
            && self.smap.mem() - self.admission_memory_limit > 0
    }
}

// Implement RefreshBackend trait
#[async_trait::async_trait]
impl crate::workers::RefreshBackend for InMemoryStorage {
    fn len(&self) -> i64 {
        self.len()
    }

    fn mem(&self) -> i64 {
        self.mem()
    }

    fn peek_expired_ttl(&self) -> Option<Entry> {
        self.peek_expired_ttl()
    }

    async fn on_ttl(&self, entry: &mut Entry) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.on_ttl_internal(entry).await
    }
}

// Implement EvictionBackend trait
impl crate::workers::EvictionBackend for InMemoryStorage {
    fn len(&self) -> i64 {
        self.len()
    }

    fn mem(&self) -> i64 {
        self.mem()
    }

    fn soft_memory_limit_overcome(&self) -> bool {
        self.soft_memory_limit_overcome()
    }

    fn soft_evict_until_within_limit(&self, backoff: i64) -> (i64, i64) {
        self.soft_evict_until_within_limit(backoff)
    }
}

// Implement Storage trait
#[async_trait::async_trait]
impl crate::storage::Storage for InMemoryStorage {
    fn get(&self, entry: &Entry) -> (Option<Entry>, bool) {
        self.get(entry)
    }

    fn get_by_key(&self, key: u64) -> (Option<Entry>, bool) {
        let entry = self.get_by_key(key);
        (entry.clone(), entry.is_some())
    }

    fn set(&self, entry: Entry) -> bool {
        self.set(entry)
    }

    fn walk_shards(&self, ctx: CancellationToken, mut f: Box<dyn FnMut(u64, &crate::storage::map::Shard<Entry>) + Send + Sync>) {
        let ctx = &ctx;
        self.smap.walk_shards(ctx, |shard_id, shard| {
            if !ctx.is_cancelled() {
                f(shard_id, shard);
            }
        });
    }

    fn remove(&self, entry: &Entry) -> (i64, bool) {
        self.remove(entry)
    }

    fn stat(&self) -> (i64, i64) {
        self.stat()
    }

    fn clear(&self) {
        self.smap.clear();
    }
}

