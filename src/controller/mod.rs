// HTTP API controllers for cache management endpoints.

pub mod admission;
pub mod backend;
pub mod bypass;
pub mod cache;
pub mod clear;
pub mod compression;
pub mod config;
pub mod controller;
pub mod evictor;
pub mod get;
pub mod invalidator;
pub mod lifetimer;
pub mod metrics;
pub mod probe;
pub mod traces;

// Re-export controller types for convenience
pub use admission::AdmissionController;
pub use backend::ChangeBackendPolicyController;
pub use bypass::BypassOnOffController;
pub use cache::CacheProxyController;
pub use clear::ClearController;
pub use compression::HttpCompressionController;
pub use config::ShowConfigController;
pub use evictor::EvictionController;
pub use get::GetController;
pub use invalidator::InvalidateController;
pub use lifetimer::LifetimeManagerController;
pub use metrics::PrometheusMetricsController;
pub use probe::LivenessProbeController;
pub use traces::TracesController;
