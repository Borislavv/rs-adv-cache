// Package sharded implements a high-throughput, zero-allocation sharded map
// intended for in-memory cache workloads.

pub mod mode;
pub mod queue;
pub mod lock;
pub mod lru;
pub mod shard;
pub mod map;
pub mod eviction;
pub mod refresh;

// Re-export main types
pub use map::Map;
pub use shard::Shard;

// Re-export NUM_OF_SHARDS for tests
#[cfg(test)]
pub use map::NUM_OF_SHARDS;

