// Package sharded implements a high-throughput, zero-allocation sharded map
// intended for in-memory cache workloads.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use tokio_util::sync::CancellationToken;

use crate::config::{Config, ConfigTrait};

use super::mode::LRUMode;
use super::shard::{Shard, Value};

/// Number of shards in the map.
pub const NUM_OF_SHARDS: usize = 1024;
pub const SHARD_MASK: u64 = (NUM_OF_SHARDS - 1) as u64;

/// Map is a sharded concurrent map with precise global counters.
pub struct Map<V: Value> {
    pub(crate) mode: LRUMode,
    shutdown_token: CancellationToken,
    pub(crate) cfg: Config,
    pub(crate) len: AtomicI64,
    pub(crate) mem: AtomicI64,
    pub(crate) iter: AtomicU64,
    pub(crate) shards: Vec<Shard<V>>,
}

impl<V: Value> Map<V> {
    /// Creates a new sharded map.
    pub fn new(shutdown_token: CancellationToken, cfg: Config) -> Self {
        let mut shards = Vec::with_capacity(NUM_OF_SHARDS);
        for id in 0..NUM_OF_SHARDS {
            shards.push(Shard::new(id as u64));
        }

        let mode = if cfg.storage().is_listing {
            LRUMode::Listing
        } else {
            LRUMode::Sampling
        };

        let map = Self {
            mode,
            shutdown_token,
            cfg,
            len: AtomicI64::new(0),
            mem: AtomicI64::new(0),
            iter: AtomicU64::new(0),
            shards,
        };

        // Enable/disable LRU based on mode
        if matches!(mode, LRUMode::Listing) {
            map.use_listing_mode();
        } else {
            map.use_sampling_mode();
        }

        map
    }

    /// Sets or updates a value.
    pub fn set(&self, key: u64, value: V) {
        let (bytes_delta, len_delta) = self.shard(key).set(key, value);
        if bytes_delta != 0 {
            self.mem.fetch_add(bytes_delta, Ordering::Relaxed);
        }
        if len_delta != 0 {
            self.len.fetch_add(len_delta, Ordering::Relaxed);
        }
    }

    /// Gets a value by key.
    pub fn get(&self, key: u64) -> Option<V>
    where
        V: Clone,
    {
        self.shard(key).get(key)
    }

    /// Removes a key.
    /// Returns (freed_bytes, hit).
    pub fn remove(&self, key: u64) -> (i64, bool)
    where
        V: Clone,
    {
        let (freed_bytes, hit) = self.shard(key).remove(key);
        if hit {
            self.len.fetch_sub(1, Ordering::Relaxed);
            self.mem.fetch_sub(freed_bytes, Ordering::Relaxed);
        }
        (freed_bytes, hit)
    }

    /// Walks over all shards synchronously.
    pub fn walk_shards<F>(&self, token: &CancellationToken, mut f: F)
    where
        F: FnMut(u64, &Shard<V>),
    {
        for (idx, shard) in self.shards.iter().enumerate() {
            if token.is_cancelled() {
                return;
            }
            f(idx as u64, shard);
        }
    }

    /// Walks over shards concurrently with bounded concurrency.
    /// Used in tests and can be used for async operations.
    pub async fn walk_shards_concurrent<F>(&self, token: &CancellationToken, concurrency: usize, f: F)
    where
        F: Fn(u64, &Shard<V>) + Send + Sync + Clone + 'static,
        V: 'static,
    {
        use std::sync::Arc;
        use tokio::sync::Semaphore;
        use futures::stream::{self, StreamExt};

        let concurrency = concurrency.max(1);
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let shards: Vec<_> = self.shards.iter().enumerate().collect();

        stream::iter(shards)
            .for_each_concurrent(concurrency, |(idx, shard)| {
                let sem = semaphore.clone();
                let token = token.clone();
                let f = f.clone();
                async move {
                    if token.is_cancelled() {
                        return;
                    }
                    let _permit = sem.acquire().await.unwrap();
                    if !token.is_cancelled() {
                        f(idx as u64, shard);
                    }
                }
            })
            .await;
    }

    /// Clears all shards.
    pub fn clear(&self) {
        self.walk_shards(&self.shutdown_token, |_, shard| {
            let (freed_bytes, items) = shard.clear();
            if freed_bytes != 0 {
                self.mem.fetch_sub(freed_bytes, Ordering::Relaxed);
            }
            if items != 0 {
                self.len.fetch_sub(items, Ordering::Relaxed);
            }
        });
    }

    /// Gets the shard for a given key.
    pub fn shard(&self, key: u64) -> &Shard<V> {
        &self.shards[(key & SHARD_MASK) as usize]
    }

    /// Gets the next shard (round-robin).
    pub fn next_shard(&self) -> &Shard<V> {
        let idx = self.iter.fetch_add(1, Ordering::Relaxed) & SHARD_MASK;
        &self.shards[idx as usize]
    }

    /// Checks if using listing mode.
    #[allow(dead_code)]
    pub fn using_listing(&self) -> bool {
        matches!(self.mode, LRUMode::Listing)
    }

    /// Checks if using sampling mode.
    #[allow(dead_code)]
    pub fn using_sampling(&self) -> bool {
        matches!(self.mode, LRUMode::Sampling)
    }

    /// Gets the number of items.
    pub fn len(&self) -> i64 {
        self.len.load(Ordering::Relaxed)
    }

    /// Gets the total memory usage in bytes.
    pub fn mem(&self) -> i64 {
        self.mem.load(Ordering::Relaxed)
    }

    /// Adds to memory counter for a specific key's shard.
    pub fn add_mem(&self, key: u64, delta: i64) {
        self.mem.fetch_add(delta, Ordering::Relaxed);
        self.shard(key).add_mem(delta);
    }

    /// Enables listing mode (full LRU).
    fn use_listing_mode(&mut self) {
        self.mode = LRUMode::Listing;
        for shard in &self.shards {
            shard.enable_lru();
        }
    }

    /// Enables sampling mode (approximate LRU).
    fn use_sampling_mode(&mut self) {
        self.mode = LRUMode::Sampling;
        for shard in &self.shards {
            shard.disable_lru();
        }
    }

    /// Touches a key (updates LRU if in listing mode).
    pub fn touch(&self, key: u64) {
        if matches!(self.mode, LRUMode::Listing) {
            self.shard(key).touch_lru(key);
        }
    }
}

