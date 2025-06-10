//! Error types for yuha-core

use thiserror::Error;

/// Result type alias for yuha-core operations
pub type Result<T> = std::result::Result<T, YuhaError>;

/// Central error type for all yuha-core operations
#[derive(Error, Debug)]
pub enum YuhaError {
    /// Clipboard-related errors
    #[error(transparent)]
    Clipboard(#[from] ClipboardError),

    /// Browser-related errors
    #[error(transparent)]
    Browser(#[from] BrowserError),

    /// Channel/communication errors
    #[error(transparent)]
    Channel(#[from] ChannelError),

    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Protocol errors
    #[error("Protocol error: {message}")]
    Protocol { message: String },

    /// Configuration errors
    #[error("Configuration error: {message}")]
    Config { message: String },

    /// Session management errors
    #[error("Session error: {message}")]
    Session { message: String },

    /// Generic internal errors
    #[error("Internal error: {message}")]
    Internal { message: String },
}

/// Clipboard operation errors
#[derive(Error, Debug)]
pub enum ClipboardError {
    /// Failed to read from clipboard
    #[error("Failed to read clipboard: {reason}")]
    ReadFailed { reason: String },

    /// Failed to write to clipboard
    #[error("Failed to write clipboard: {reason}")]
    WriteFailed { reason: String },

    /// Clipboard access denied
    #[error("Clipboard access denied")]
    AccessDenied,

    /// Lock acquisition failed
    #[error("Failed to acquire clipboard lock: {reason}")]
    LockFailed { reason: String },

    /// Unsupported clipboard format
    #[error("Unsupported clipboard format: {format}")]
    UnsupportedFormat { format: String },
}

/// Browser operation errors
#[derive(Error, Debug)]
pub enum BrowserError {
    /// Failed to open URL
    #[error("Failed to open URL '{url}': {reason}")]
    OpenFailed { url: String, reason: String },

    /// Invalid URL format
    #[error("Invalid URL format: {url}")]
    InvalidUrl { url: String },

    /// No browser available
    #[error("No default browser available")]
    NoBrowserAvailable,

    /// Browser execution failed
    #[error("Browser execution failed: {reason}")]
    ExecutionFailed { reason: String },
}

/// Channel/communication errors (re-exported from message_channel)
#[derive(Error, Debug)]
pub enum ChannelError {
    /// I/O error during communication
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Protocol-level error
    #[error("Protocol error: {message}")]
    Protocol { message: String },

    /// Channel is closed
    #[error("Channel closed")]
    Closed,

    /// Serialization/deserialization error
    #[error("Serialization error: {reason}")]
    Serialization { reason: String },

    /// Timeout occurred
    #[error("Operation timed out after {seconds} seconds")]
    Timeout { seconds: u64 },

    /// Buffer overflow
    #[error("Buffer overflow: message too large ({size} bytes)")]
    BufferOverflow { size: usize },
}

impl YuhaError {
    /// Create a new protocol error
    pub fn protocol<S: Into<String>>(message: S) -> Self {
        Self::Protocol {
            message: message.into(),
        }
    }

    /// Create a new configuration error
    pub fn config<S: Into<String>>(message: S) -> Self {
        Self::Config {
            message: message.into(),
        }
    }

    /// Create a new session error
    pub fn session<S: Into<String>>(message: S) -> Self {
        Self::Session {
            message: message.into(),
        }
    }

    /// Create a new internal error
    pub fn internal<S: Into<String>>(message: S) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// Check if this is a retriable error
    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            YuhaError::Io(_)
                | YuhaError::Channel(ChannelError::Timeout { .. })
                | YuhaError::Channel(ChannelError::Io(_))
                | YuhaError::Browser(BrowserError::ExecutionFailed { .. })
        )
    }

    /// Get the error category for logging/metrics
    pub fn category(&self) -> &'static str {
        match self {
            YuhaError::Clipboard(_) => "clipboard",
            YuhaError::Browser(_) => "browser",
            YuhaError::Channel(_) => "channel",
            YuhaError::Io(_) => "io",
            YuhaError::Protocol { .. } => "protocol",
            YuhaError::Config { .. } => "config",
            YuhaError::Session { .. } => "session",
            YuhaError::Internal { .. } => "internal",
        }
    }
}

impl ClipboardError {
    /// Create a read failed error
    pub fn read_failed<S: Into<String>>(reason: S) -> Self {
        Self::ReadFailed {
            reason: reason.into(),
        }
    }

    /// Create a write failed error
    pub fn write_failed<S: Into<String>>(reason: S) -> Self {
        Self::WriteFailed {
            reason: reason.into(),
        }
    }

    /// Create a lock failed error
    pub fn lock_failed<S: Into<String>>(reason: S) -> Self {
        Self::LockFailed {
            reason: reason.into(),
        }
    }
}

impl BrowserError {
    /// Create an open failed error
    pub fn open_failed<S: Into<String>, R: Into<String>>(url: S, reason: R) -> Self {
        Self::OpenFailed {
            url: url.into(),
            reason: reason.into(),
        }
    }

    /// Create an invalid URL error
    pub fn invalid_url<S: Into<String>>(url: S) -> Self {
        Self::InvalidUrl { url: url.into() }
    }

    /// Create an execution failed error
    pub fn execution_failed<S: Into<String>>(reason: S) -> Self {
        Self::ExecutionFailed {
            reason: reason.into(),
        }
    }
}

impl ChannelError {
    /// Create a protocol error
    pub fn protocol<S: Into<String>>(message: S) -> Self {
        Self::Protocol {
            message: message.into(),
        }
    }

    /// Create a serialization error
    pub fn serialization<S: Into<String>>(reason: S) -> Self {
        Self::Serialization {
            reason: reason.into(),
        }
    }

    /// Create a timeout error
    pub fn timeout(seconds: u64) -> Self {
        Self::Timeout { seconds }
    }

    /// Create a buffer overflow error
    pub fn buffer_overflow(size: usize) -> Self {
        Self::BufferOverflow { size }
    }
}

// Implement conversion from anyhow::Error for backward compatibility
impl From<anyhow::Error> for YuhaError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal {
            message: err.to_string(),
        }
    }
}

// Note: anyhow::Error already implements From<E> for any E: std::error::Error
// so we don't need to implement it manually
