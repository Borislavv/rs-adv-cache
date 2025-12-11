// Package liveness provides configuration for liveness probes.
//

use std::time::Duration;
use tracing::warn;

/// Configuration for liveness probe.
#[derive(Debug, Clone)]
pub struct Config {
    /// Timeout duration as a string (e.g., "5s", "10ms").
    pub timeout: String,
}

impl Config {
    /// Creates a new config with default timeout.
    pub fn new() -> Self {
        Self {
            timeout: "5s".to_string(),
        }
    }

    /// Parses and returns the liveness timeout duration.
    pub fn liveness_timeout(&self) -> Duration {
        humantime::parse_duration(&self.timeout).unwrap_or_else(|err| {
            warn!(
                error = %err,
                timeout = %self.timeout,
                "Failed to parse liveness probe timeout, using default: 5s"
            );
            Duration::from_secs(5)
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

