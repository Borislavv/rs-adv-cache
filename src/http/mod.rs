// HTTP module: server, client, header/query helpers, rendering, utils.

pub mod client;
pub mod header;
pub mod query;
pub mod render;
pub mod server;
pub mod utils;

// Re-export middleware interfaces from the dedicated middleware crate module
pub use crate::middleware::compression_middleware::{
    disable_compression, enable_compression, is_compression_enabled,
};
pub use crate::middleware::middleware::Middleware;
pub use crate::middleware::recover_middleware::panics_counter;

// Re-export server types
pub use server::{HttpServer, Server};

// Common controller interface
pub use crate::controller::controller::Controller;
