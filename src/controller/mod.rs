// HTTP API controllers for cache management endpoints.

pub mod probe;
pub mod metrics;
pub mod bypass;
pub mod config;
pub mod clear;
pub mod get;
pub mod compression;
pub mod backend;
pub mod admission;
pub mod traces;
pub mod evictor;
pub mod lifetimer;
pub mod cache;
pub mod invalidator;
pub mod controller;

// Re-export controller types for convenience
pub use probe::LivenessProbeController;
pub use metrics::PrometheusMetricsController;
pub use bypass::BypassOnOffController;
pub use config::ShowConfigController;
pub use clear::ClearController;
pub use get::GetController;
pub use compression::HttpCompressionController;
pub use backend::ChangeBackendPolicyController;
pub use admission::AdmissionController;
pub use traces::TracesController;
pub use evictor::EvictionController;
pub use lifetimer::LifetimeManagerController;
pub use cache::CacheProxyController;
pub use invalidator::InvalidateController;
