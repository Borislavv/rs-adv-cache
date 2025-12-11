// Package lfu provides LFU (Least Frequently Used) admission control.

pub mod admission;
pub mod count_min_sketch;
pub mod door_keeper;
pub mod helper;
pub mod tiny_lfu;

#[cfg(test)]
mod tiny_lfu_test;
#[cfg(test)]
mod admission_test;

// Re-export main types
pub use admission::{Admission, new_admission};

