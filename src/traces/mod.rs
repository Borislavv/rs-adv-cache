pub mod tracer;

// Re-export commonly used functions and constants
pub use tracer::{
    is_active_tracing, enable_tracing, disable_tracing,
    ATTR_HTTP_STATUS_CODE_KEY, ATTR_HTTP_RESPONSE_SIZE_KEY,
    ATTR_CACHE_PROXY, ATTR_CACHE_HIT, ATTR_CACHE_KEY,
    ATTR_CACHE_IS_ERR, extract,
};

use anyhow::Result;
use tokio_util::sync::CancellationToken;
use crate::config::Traces;

/// Applies tracing configuration and returns a shutdown function.
pub fn apply(
    shutdown_token: CancellationToken,
    cfg: Option<Traces>,
) -> Box<dyn Fn(CancellationToken) -> Result<()> + Send + Sync> {
    tracer::apply(shutdown_token, cfg)
}

