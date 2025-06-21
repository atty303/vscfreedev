//! # Legacy Protocol Re-exports (Deprecated)
//!
//! **⚠️ DEPRECATED**: This module maintains backward compatibility while the codebase migrates
//! to the new unified protocol abstraction. New code should use the protocol types from
//! `protocol::simple` or `protocol::daemon` directly.
//!
//! ## Migration Guide
//!
//! Instead of using legacy type aliases:
//!
//! ```rust,ignore
//! use yuha_core::protocol_legacy::{YuhaRequest, YuhaResponse};
//! ```
//!
//! Use the new protocol types directly:
//!
//! ```rust,no_run
//! use yuha_core::protocol::simple::{SimpleRequest, SimpleResponse};
//! ```
//!
//! This module will be removed in a future version.

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
