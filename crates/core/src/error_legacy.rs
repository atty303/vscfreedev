//! Legacy error types for backward compatibility
//!
//! This module provides backward compatibility while migrating to the new
//! unified error system. New code should use the error types from error::mod.

// Re-export the new unified error system
pub use self::mod_new::*;

// Legacy type aliases for backward compatibility
pub use self::mod_new::{
    BrowserError,
    ClipboardError,
    ProtocolError as ChannelError,
};

// Import the new error system
#[path = "error/mod.rs"]
mod mod_new;
