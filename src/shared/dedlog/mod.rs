//! Deduplicated logging functionality to prevent log spam.

pub mod consts;
pub mod sanitizer;
pub mod log_entry;

pub use log_entry::{err, start_dedup_logger};