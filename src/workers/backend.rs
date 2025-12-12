// Package workers exposes backend interfaces used by worker groups.

use crate::model::Entry;
use anyhow::Result;

/// EvictionBackend interface for eviction operations.
pub trait EvictionBackend: Send + Sync {
    /// Gets the number of entries.
    #[allow(dead_code)]
    fn len(&self) -> i64;

    /// Gets the memory usage in bytes.
    #[allow(dead_code)]
    fn mem(&self) -> i64;

    /// Checks if soft memory limit is overcome.
    #[allow(dead_code)] // Used in evictor workers
    fn soft_memory_limit_overcome(&self) -> bool;

    /// Evicts entries until within soft memory limit.
    #[allow(dead_code)] // Used in evictor workers
    fn soft_evict_until_within_limit(&self, backoff: i64) -> (i64, i64);
}

/// RefreshBackend interface for refresh operations.
#[async_trait::async_trait]
pub trait RefreshBackend: Send + Sync {
    /// Gets the number of entries.
    fn len(&self) -> i64;

    /// Gets the memory usage in bytes.
    fn mem(&self) -> i64;

    /// Peeks at an expired entry (without removing it).
    fn peek_expired_ttl(&self) -> Option<Entry>;

    /// Handles TTL expiration for an entry.
    async fn on_ttl(&self, entry: &mut Entry) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

