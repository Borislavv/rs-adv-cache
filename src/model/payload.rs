// Package model provides payload operations.

use crate::bytes;
use super::Entry;

// Re-export constants from payload_encoder
pub use super::payload_encoder::{OFFSETS_MAP_SIZE, OFF_QUERY, OFF_REQ_HDRS, OFF_WEIGHT};

impl Entry {
    /// Gets the weight of the entry (size of struct + payload capacity).
    pub fn weight(&self) -> i64 {
        let struct_size = std::mem::size_of::<Entry>() as i64;
        let payload_cap = self.payload_bytes().len() as i64;
        struct_size + payload_cap
    }

    /// Swaps payloads between two entries and returns weight difference.
    pub fn swap_payloads(&mut self, other: &mut Entry) -> i64 {
        let new_weight = other.weight();
        let old_weight = self.weight();
        
        // Swap payloads
        let self_payload = self.payload.lock().unwrap().take();
        let other_payload = other.payload.lock().unwrap().take();
        
        *self.payload.lock().unwrap() = other_payload;
        *other.payload.lock().unwrap() = self_payload;
        
        new_weight - old_weight
    }

    /// Checks if two entries have the same payload.
    pub fn is_the_same_payload(&self, other: &Entry) -> bool {
        let a = self.payload_bytes();
        let b = other.payload_bytes();
        
        if a.is_empty() {
            return b.is_empty();
        }
        if b.is_empty() {
            return false;
        }
        
        bytes::is_bytes_are_equals(&a, &b)
    }

    /// Gets the payload bytes.
    pub fn payload_bytes(&self) -> Vec<u8> {
        if let Ok(payload) = self.payload.lock() {
            payload.clone().unwrap_or_default()
        } else {
            Vec::new()
        }
    }
}

