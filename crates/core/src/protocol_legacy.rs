//! Legacy protocol re-exports for backward compatibility
//!
//! This module maintains backward compatibility while the codebase migrates
//! to the new unified protocol abstraction.

// Re-export the new protocol abstractions
pub use self::mod_new::*;

// Re-export specific protocol implementations
pub use self::simple::{ResponseItem, SimpleRequest as YuhaRequest, SimpleResponse as YuhaResponse};
pub use self::buffer::SimpleResponseBuffer as ResponseBuffer;

// Import the new modular protocol system
#[path = "protocol/mod.rs"]
mod mod_new;
#[path = "protocol/simple.rs"] 
mod simple;
#[path = "protocol/buffer.rs"]
mod buffer;
