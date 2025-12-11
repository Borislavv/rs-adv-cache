// Package orchestrator provides service interfaces.
//

use std::sync::Arc;
use std::time::Duration;

use super::transport::Transport;

/// Freq interface for frequency/rate limiting configuration.
pub trait Freq: Send + Sync {
    /// Checks if rate limit is defined.
    #[allow(dead_code)]
    fn is_rate_limit_defined(&self) -> bool;

    /// Checks if tick frequency is defined.
    #[allow(dead_code)]
    fn is_tick_freq_defined(&self) -> bool;

    /// Gets the rate limit.
    fn get_rate_limit(&self) -> usize;

    /// Sets the rate limit and returns a new Freq.
    fn set_rate_limit(&self, new: usize) -> Arc<dyn Freq>;

    /// Gets the tick frequency.
    fn get_tick_freq(&self) -> Duration;

    /// Sets the tick frequency and returns a new Freq.
    #[allow(dead_code)]
    fn set_tick_freq(&self, new: Duration) -> Arc<dyn Freq>;

    /// Clones the Freq.
    fn clone_freq(&self) -> Arc<dyn Freq>;
}

/// Config is not a thread-safe structure, so it ought to be cloned before mutation.
pub trait Config: Send + Sync {
    /// Checks if the service is enabled.
    fn is_enabled(&self) -> bool;

    /// Sets the enabled state and returns a new Config.
    fn set_enabled(&self, v: bool) -> Arc<dyn Config>;

    /// Gets the number of replicas.
    fn get_replicas(&self) -> usize;

    /// Sets the number of replicas and returns a new Config.
    fn set_replicas(&self, n: usize) -> Arc<dyn Config>;

    /// Gets the frequency configuration.
    fn get_freq(&self) -> Arc<dyn Freq>;

    /// Sets the frequency configuration and returns a new Config.
    fn set_freq(&self, freq: Arc<dyn Freq>) -> Arc<dyn Config>;

    /// Clones the Config.
    #[allow(dead_code)]
    fn clone_config(&self) -> Arc<dyn Config>;
}

/// Service is implemented by a worker group (evictor, refresher, logger, ...).
pub trait Service: Send + Sync {
    /// Gets the service name.
    fn name(&self) -> &str;

    /// Gets the service configuration.
    fn cfg(&self) -> Arc<dyn Config>;

    /// Gets the number of replicas.
    #[allow(dead_code)]
    fn replicas(&self) -> usize;

    /// Serves the service with the given transport.
    fn serve(&self, t: Arc<dyn Transport>);

    /// Gets the transport for the service.
    fn transport(&self) -> Arc<dyn Transport>;
}

