//! Prometheus metrics functionality.
//
//! Metrics organization:
//! - Custom cache metrics: controller::metrics (cache_memory_usage, cache_hits, etc.)
//! - Process metrics: metrics-process (process_resident_memory_bytes, process_cpu_*, etc.)

pub mod code;
pub mod meter;
pub mod policy;

// Re-export commonly used items
pub use code::*;
pub use meter::*;
