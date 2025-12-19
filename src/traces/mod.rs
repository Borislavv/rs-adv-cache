pub mod tracer;

// Re-export commonly used functions and constants
pub use tracer::{
    disable_tracing, enable_tracing, extract, is_active_tracing, ATTR_CACHE_HIT, ATTR_CACHE_IS_ERR,
    ATTR_CACHE_KEY, ATTR_CACHE_PROXY,
    ATTR_HTTP_RESPONSE_SIZE_KEY, ATTR_HTTP_STATUS_CODE_KEY,
};

use crate::config::Traces;
use anyhow::Result;
use tokio_util::sync::CancellationToken;

/// Applies tracing configuration and returns a shutdown function.
pub fn apply(
    shutdown_token: CancellationToken,
    cfg: Option<Traces>,
) -> Box<dyn Fn(CancellationToken) -> Result<()> + Send + Sync> {
    tracer::apply(shutdown_token, cfg)
}
