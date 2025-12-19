//! Lifetime management worker group.

pub mod counters;
pub mod lifetimer;
pub mod telemetry;

// Re-export main types
pub use lifetimer::LifetimeManager;
