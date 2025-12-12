// Package sharded provides shard implementation.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use parking_lot::RwLock;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::model::Entry;

use super::queue::Queue;
use super::lru::LRUList;

/// Value trait for items stored in the sharded map.
/// All methods must be O(1) and allocation-free where possible.
pub trait Value: Send + Sync + Clone {
    fn key(&self) -> u64;
    fn weight(&self) -> i64;
    fn is_expired(&self, cfg: &Config) -> bool;
    fn clear_refresh_queued(&self);
    fn touched_at(&self) -> i64;
    fn fresh_at(&self) -> i64;
}

impl Value for Entry {
    fn key(&self) -> u64 {
        self.key()
    }

    fn weight(&self) -> i64 {
        self.weight()
    }

    fn is_expired(&self, cfg: &Config) -> bool {
        self.is_expired(cfg)
    }

    fn clear_refresh_queued(&self) {
        self.clear_refresh_queued();
    }

    fn touched_at(&self) -> i64 {
        self.touched_at()
    }

    fn fresh_at(&self) -> i64 {
        self.fresh_at()
    }
}

/// Shard data protected by lock.
pub struct ShardData<V: Value> {
    pub(crate) items: HashMap<u64, V>,
    lru: Option<LRUList>,
    lru_on: bool,
}

/// Shard is an independent segment of the sharded map.
pub struct Shard<V: Value> {
    pub(crate) data: RwLock<ShardData<V>>,
    #[allow(dead_code)]
    id: u64,
    mem: AtomicI64,
    len: AtomicI64,
    #[allow(dead_code)]
    rand_iter: AtomicU64,
    rq: Queue,
}

impl<V: Value> Shard<V> {
    /// Creates a new shard.
    pub fn new(id: u64) -> Self {
        Self {
            data: RwLock::new(ShardData {
                items: HashMap::new(),
                lru: None,
                lru_on: false,
            }),
            id,
            mem: AtomicI64::new(0),
            len: AtomicI64::new(0),
            rand_iter: AtomicU64::new(0),
            rq: Queue::default(),
        }
    }

    /// Gets the shard ID.
    /// Part of public API, may be used for diagnostics or tests.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Gets the total weight in bytes.
    /// Used in eviction and other internal operations.
    pub fn weight(&self) -> i64 {
        self.mem.load(Ordering::Relaxed)
    }

    /// Gets the number of items.
    pub fn len(&self) -> i64 {
        self.len.load(Ordering::Relaxed)
    }

    /// Adds to memory counter.
    pub fn add_mem(&self, delta: i64) {
        self.mem.fetch_add(delta, Ordering::Relaxed);
    }

    /// Sets or updates a key-value pair.
    /// Returns (bytes_delta, len_delta).
    pub fn set(&self, key: u64, new_value: V) -> (i64, i64) {
        let mut data = self.data.write();
        let new_weight = new_value.weight();
        
        if let Some(old_value) = data.items.get(&key) {
            let old_weight = old_value.weight();
            data.items.insert(key, new_value);
            if data.lru_on {
                if let Some(ref mut lru) = data.lru {
                    lru.move_to_front(key);
                }
            }
            
            let bytes_delta = new_weight - old_weight;
            self.mem.fetch_add(bytes_delta, Ordering::Relaxed);
            (bytes_delta, 0)
        } else {
            data.items.insert(key, new_value);
            if data.lru_on {
                if let Some(ref mut lru) = data.lru {
                    lru.push_front(key);
                }
            }
            
            self.len.fetch_add(1, Ordering::Relaxed);
            self.mem.fetch_add(new_weight, Ordering::Relaxed);
            (new_weight, 1)
        }
    }

    /// Gets a value by key.
    pub fn get(&self, key: u64) -> Option<V>
    where
        V: Clone,
    {
        self.data.read().items.get(&key).cloned()
    }

    /// Removes a key and returns (freed_bytes, hit).
    pub fn remove(&self, key: u64) -> (i64, bool)
    where
        V: Clone,
    {
        let mut data = self.data.write();
        if let Some(old_value) = data.items.remove(&key) {
            if data.lru_on {
                if let Some(ref mut lru) = data.lru {
                    lru.remove(key);
                }
            }
            let freed_bytes = old_value.weight();
            self.mem.fetch_sub(freed_bytes, Ordering::Relaxed);
            self.len.fetch_sub(1, Ordering::Relaxed);
            (freed_bytes, true)
        } else {
            (0, false)
        }
    }

    /// Clears all entries.
    /// Returns (freed_bytes, items_removed).
    pub fn clear(&self) -> (i64, i64) {
        let mut data = self.data.write();
        let items_count = self.len.load(Ordering::Relaxed);
        let freed_bytes = self.mem.load(Ordering::Relaxed);
        
        data.items.clear();
        if let Some(ref mut lru) = data.lru {
            lru.clear();
        }
        
        self.len.store(0, Ordering::Relaxed);
        self.mem.store(0, Ordering::Relaxed);
        
        (freed_bytes, items_count)
    }

    /// Enqueues a key for refresh.
    pub fn enqueue_refresh(&self, key: u64) -> bool {
        self.rq.try_push(key)
    }

    /// Dequeues an expired key.
    pub fn dequeue_expired(&self) -> Option<u64> {
        self.rq.try_pop()
    }

    /// Enables LRU tracking.
    pub fn enable_lru(&self) {
        let mut data = self.data.write();
        if data.lru.is_none() {
            let mut lru = LRUList::new();
            // Initialize LRU with existing items
            for &key in data.items.keys() {
                lru.push_front(key);
            }
            data.lru = Some(lru);
        }
        data.lru_on = true;
    }

    /// Disables LRU tracking.
    pub fn disable_lru(&self) {
        let mut data = self.data.write();
        data.lru_on = false;
        if let Some(ref mut lru) = data.lru {
            lru.clear();
        }
    }

    /// Touches LRU (thread-safe, tries lock).
    pub fn touch_lru(&self, key: u64) {
        if let Some(mut data) = self.data.try_write() {
            if data.lru_on {
                if let Some(ref mut lru) = data.lru {
                    lru.move_to_front(key);
                }
            }
        }
    }

    /// Peeks at the LRU tail.
    pub fn lru_peek_tail(&self) -> Option<u64> {
        let data = self.data.read();
        if data.lru_on {
            data.lru.as_ref().and_then(|lru| {
                // Check if empty before peeking (as done in eviction checks)
                if lru.is_empty() {
                    None
                } else {
                    lru.peek_tail()
                }
            })
        } else {
            None
        }
    }

    /// Pops the LRU tail.
    pub fn lru_pop_tail(&self) -> Option<(u64, V)>
    where
        V: Clone,
    {
        let mut data = self.data.write();
        if !data.lru_on {
            return None;
        }
        let lru = match &mut data.lru {
            Some(l) => l,
            None => return None,
        };
        
        // Get tail key from LRU
        let key = match lru.peek_tail() {
            Some(k) => k,
            None => return None,
        };
        
        // Get value from items
        let value = match data.items.get(&key).cloned() {
            Some(v) => v,
            None => {
                // Item was already removed, clean up LRU
                lru.pop_tail();
                return None;
            }
        };
        
        // Remove from items
        data.items.remove(&key);
        
        let weight = value.weight();
        self.len.fetch_sub(1, Ordering::Relaxed);
        self.mem.fetch_sub(weight, Ordering::Relaxed);
        
        // Remove from LRU
        lru.pop_tail();
        
        Some((key, value))
    }

    /// Walks over items with a read lock.
    pub fn walk_r<F>(&self, token: &CancellationToken, mut f: F)
    where
        F: FnMut(u64, &V) -> bool,
        V: Clone,
    {
        if token.is_cancelled() {
            return;
        }
        let data = self.data.read();
        for (k, v) in data.items.iter() {
            if token.is_cancelled() {
                return;
            }
            if !f(*k, v) {
                return;
            }
        }
    }

}
