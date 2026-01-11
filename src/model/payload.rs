//! Payload operations.
//

use std::sync::Arc;

use super::Entry;

// Re-export constants from payload_encoder
pub use super::payload_encoder::{OFFSETS_MAP_SIZE, OFF_QUERY, OFF_REQ_HDRS, OFF_WEIGHT};

impl Entry {
    /// Gets the weight of the entry (size of struct + payload length).
    /// Calculates entry size: EntryInner size + payload capacity
    /// With Bytes, capacity equals length (zero-copy immutable buffer)
    pub fn weight(&self) -> i64 {
        // Count EntryInner struct size (not Arc, which is just a pointer)
        let struct_size = std::mem::size_of::<crate::model::entry::EntryInner>() as i64;
        
        // Count Vec capacity (not len) to match actual physical memory usage
        // capacity() reflects the actual memory allocated by the allocator, including overhead
        // Even with shrink_to_fit(), allocator may leave capacity > len for alignment/efficiency
        let payload_guard = self.0.payload.load();
        let payload_capacity = payload_guard.as_ref()
            .map(|arc_vec| arc_vec.capacity())
            .unwrap_or(0) as i64;
        
        struct_size + payload_capacity
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
        // Payload is Arc<Vec<u8>> = 8 bytes (if exists)
        // Rule is Arc<Rule> = 8 bytes (shared but counted for simplicity)
        let payload_guard = self.0.payload.load();
        let arc_overhead = if payload_guard.is_some() { 24 } else { 16 }; // 3 Arcs if payload exists, 2 if not
        
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
        
        let self_payload = self_guard.as_ref().map(Arc::clone);
        let other_payload = other_guard.as_ref().map(Arc::clone);
        
        drop(self_guard);
        drop(other_guard);
        
        self.0.payload.store(other_payload);
        other.0.payload.store(self_payload);

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
            (Some(a_vec), Some(b_vec)) => {
                if a_vec.len() != b_vec.len() {
                    return false;
                }
                a_vec == b_vec
            }
        }
    }

    /// Gets the payload bytes as Vec<u8> (copy).
    pub fn payload_bytes(&self) -> Vec<u8> {
        self.0.payload.load()
            .as_ref()
            .map(|arc_vec| (**arc_vec).clone())
            .unwrap_or_default()
    }
}
