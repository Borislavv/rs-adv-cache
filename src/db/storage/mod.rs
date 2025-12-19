//! High-throughput, zero-allocation sharded map for in-memory cache workloads.

pub mod eviction;
pub mod lock;
pub mod lru;

pub mod map;
pub mod mode;
pub mod queue;
pub mod refresh;
pub mod shard;
pub mod storage;

#[cfg(test)]
mod shard_test;
#[cfg(test)]
mod storage_test;

// Re-export main types
pub use map::Map;
pub use shard::Shard;
pub use storage::Storage;

// Re-export NUM_OF_SHARDS for tests
#[cfg(test)]
pub use map::NUM_OF_SHARDS;
