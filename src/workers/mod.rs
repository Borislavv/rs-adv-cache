// Worker functionality for cache management.

pub mod backend;
pub mod config;
pub mod evictor;
pub mod lifetimer;

// Re-export main types
pub use backend::{EvictionBackend, RefreshBackend};
pub use config::{CallFreq, WorkerConfig};
