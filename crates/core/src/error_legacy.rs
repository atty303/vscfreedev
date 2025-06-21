//! # Legacy Error Types (Deprecated)
//!
//! **⚠️ DEPRECATED**: This module provides backward compatibility while migrating to the new
//! unified error system. New code should use the error types from `error::mod`.
//!
//! ## Migration Guide
//!
//! Instead of importing from this module:
//!
//! ```rust,ignore
//! use yuha_core::error_legacy::YuhaError;
//! ```
//!
//! Import from the new error module:
//!
//! ```rust,no_run
//! use yuha_core::error::YuhaError;
//! ```
//!
//! This module will be removed in a future version.

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
