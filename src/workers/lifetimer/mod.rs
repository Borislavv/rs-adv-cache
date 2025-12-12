// Package lifetimer provides lifetime management worker group.

pub mod lifetimer;
pub mod counters;
pub mod telemetry;

// Re-export main types
pub use lifetimer::LifetimeManager;

