// Package sharded provides LRU mode enum.

/// LRU eviction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LRUMode {
    /// Listing mode: uses full LRU list for precise eviction.
    Listing,
    /// Sampling mode: uses sampling for approximate eviction.
    Sampling,
}

