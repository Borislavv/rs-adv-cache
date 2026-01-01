//! Payload operations.
//

use std::sync::Arc;
use bytes::Bytes;

use super::Entry;

// Re-export constants from payload_encoder
pub use super::payload_encoder::{OFFSETS_MAP_SIZE, OFF_QUERY, OFF_REQ_HDRS, OFF_WEIGHT};

impl Entry {
    /// Gets the weight of the entry (size of struct + payload length).
    /// Calculates entry size: EntryInner size + payload capacity
    /// With Bytes, capacity equals length (zero-copy immutable buffer)
    pub fn weight(&self) -> i64 {
        // Count only EntryInner struct size (not Arc, which is just a pointer)
        let struct_size = std::mem::size_of::<crate::model::entry::EntryInner>() as i64;
        // With Vec<u8>, len() is the actual used size, but capacity may be larger
        // We count len() for logical weight, capacity overhead is accounted separately
        let payload_guard = self.0.payload.load();
        let payload_len = payload_guard.as_ref()
            .map(|arc_vec| arc_vec.len())
            .unwrap_or(0) as i64;
        struct_size + payload_len
    }

    /// Gets the estimated physical memory weight including overheads.
    /// This accounts for HashMap buckets, LRU nodes (if in listing mode), and Arc overhead.
    /// 
    /// Overhead breakdown:
    /// - HashMap entry: ~32 bytes (key 8 + value pointer 8 + hash 8 + bucket overhead ~8)
    /// - LRU links (listing mode): ~24 bytes (prev 8 + next 8 + in_lru 1 + padding)
    /// - Arc<EntryInner> pointer: 8 bytes
    /// - Arc<Bytes> pointer: 8 bytes (if payload exists)
    /// - Arc<Rule> pointer: 8 bytes (shared, but counted per entry for simplicity)
    /// - Memory alignment: ~8-16 bytes
    #[allow(dead_code)]
    pub fn weight_with_overhead(&self, is_listing_mode: bool) -> i64 {
        let base_weight = self.weight();
        
        // HashMap overhead: each entry in HashMap<u64, Entry> has overhead
        // Rust HashMap stores: key (8) + value pointer (8) + hash (8) + bucket metadata (~8)
        const HASHMAP_ENTRY_OVERHEAD: i64 = 32;
        
        // LRU overhead (only in listing mode)
        // LRU links: ~24 bytes (prev 8 + next 8 + in_lru 1 + padding)
        // Stored directly in EntryInner, no separate allocation or HashMap
        const LRU_LINKS_OVERHEAD: i64 = 24;
        
        // Arc pointer overheads (8 bytes per Arc)
        // Entry itself is Arc<EntryInner> = 8 bytes
        // Payload is Bytes (no Arc needed, Bytes handles sharing internally) = 0 bytes Arc overhead
        // Rule is Arc<Rule> = 8 bytes (shared but counted for simplicity)
        let payload_guard = self.0.payload.load();
        let arc_overhead = if payload_guard.is_some() { 16 } else { 16 }; // 2 Arcs (EntryInner + Rule)
        
        // Memory alignment/padding overhead (typical 8-16 bytes)
        const ALIGNMENT_OVERHEAD: i64 = 12;
        
        let lru_overhead = if is_listing_mode { LRU_LINKS_OVERHEAD } else { 0 };
        
        base_weight + HASHMAP_ENTRY_OVERHEAD + lru_overhead + arc_overhead + ALIGNMENT_OVERHEAD
    }

    /// Swaps payloads between two entries and returns weight difference.
    /// 
    /// Optimized to avoid unnecessary Arc clones that could cause memory leaks.
    /// Uses ArcSwapOption::swap for atomic swap without temporary clones.
    pub fn swap_payloads(&self, other: &Entry) -> i64 {
        let new_weight = other.weight();
        let old_weight = self.weight();

        // Load current payloads
        let self_guard = self.0.payload.load();
        let other_guard = other.0.payload.load();
        
        // Arc::clone() is cheap (just increments ref count)
        let self_payload = self_guard.as_ref().map(|arc_bytes| Arc::clone(arc_bytes));
        let other_payload = other_guard.as_ref().map(|arc_bytes| Arc::clone(arc_bytes));
        
        drop(self_guard);
        drop(other_guard);
        
        self.0.payload.store(self_payload);
        other.0.payload.store(other_payload);

        new_weight - old_weight
    }

    /// Checks if two entries have the same payload.
    pub fn is_the_same_payload(&self, other: &Entry) -> bool {
        let a_guard = self.0.payload.load();
        let b_guard = other.0.payload.load();
        
        let a = a_guard.as_ref();
        let b = b_guard.as_ref();

        match (a, b) {
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
            (Some(a_arc_bytes), Some(b_arc_bytes)) => {
                if a_arc_bytes.len() != b_arc_bytes.len() {
                    return false;
                }
                a_arc_bytes == b_arc_bytes
            }
        }
    }

    /// Gets the payload bytes as Bytes (zero-copy if possible).
    pub fn payload_bytes(&self) -> Bytes {
        self.0.payload.load()
            .as_ref()
            .map(|arc_bytes| arc_bytes.as_ref().clone()) // Clone Bytes from Arc (cheap, just ref count increment)
            .unwrap_or_else(|| Bytes::new())
    }
    
    /// Gets the payload bytes as Vec<u8> (copy - use only when necessary).
    pub fn payload_bytes_vec(&self) -> Vec<u8> {
        self.0.payload.load()
            .as_ref()
            .map(|bytes| bytes.to_vec())
            .unwrap_or_default()
    }
}
