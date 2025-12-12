// Cache storage functionality.

pub mod storage;
pub mod dumper;
pub mod lru;
pub mod map;
pub mod lfu;

// Re-export main types
pub use storage::{DB, Storage, SVC_EVICTOR, SVC_LIFETIME_MANAGER};

// Mock functions for testing.
pub mod mocks {
    use crate::config::Config;
    use tokio_util::sync::CancellationToken;
    
    use super::Storage;
    use std::sync::Arc;

    pub fn load_mocks(
        ctx: CancellationToken,
        cfg: Config,
        storage: Arc<dyn Storage>,
        num: usize,
    ) {
        super::storage::load_mocks(ctx, cfg, storage, num);
    }
}

