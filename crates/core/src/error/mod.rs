//! Unified error handling for the yuha ecosystem
//!
//! This module provides a centralized error handling system that consolidates
//! all error types across different components and crates.

use thiserror::Error;

pub mod categories;
pub mod context;
pub mod retry;

/// Result type alias for all yuha operations
pub type Result<T> = std::result::Result<T, YuhaError>;

/// Central error type for all yuha operations
#[derive(Error, Debug)]
pub enum YuhaError {
    /// Transport-related errors (SSH, Local, TCP, etc.)
    #[error(transparent)]
    Transport(#[from] TransportError),

    /// Protocol communication errors
    #[error(transparent)]
    Protocol(#[from] ProtocolError),

    /// Session management errors
    #[error(transparent)]
    Session(#[from] SessionError),

    /// Configuration errors
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// I/O errors
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Clipboard-related errors
    #[error(transparent)]
    Clipboard(#[from] ClipboardError),

    /// Browser-related errors
    #[error(transparent)]
    Browser(#[from] BrowserError),

    /// Authentication/authorization errors
    #[error(transparent)]
    Auth(#[from] AuthError),

    /// Daemon-specific errors
    #[error(transparent)]
    Daemon(#[from] DaemonError),

    /// Generic internal errors
    #[error("Internal error: {message}")]
    Internal { message: String },
}

/// Transport-related errors
#[derive(Error, Debug)]
pub enum TransportError {
    /// Connection failed
    #[error("Connection failed: {reason}")]
    ConnectionFailed { reason: String },

    /// Authentication failed
    #[error("Authentication failed: {reason}")]
    AuthenticationFailed { reason: String },

    /// Transport not available
    #[error("Transport '{transport_type}' not available: {reason}")]
    NotAvailable {
        transport_type: String,
        reason: String,
    },

    /// Configuration error
    #[error("Transport configuration error: {reason}")]
    ConfigurationError { reason: String },

    /// SSH-specific errors
    #[error(transparent)]
    Ssh(#[from] SshError),

    /// Local process errors
    #[error(transparent)]
    Local(#[from] LocalError),

    /// TCP connection errors
    #[error(transparent)]
    Tcp(#[from] TcpError),

    /// WSL-specific errors
    #[error(transparent)]
    Wsl(#[from] WslError),
}

/// SSH transport errors
#[derive(Error, Debug)]
pub enum SshError {
    /// Key file not found or invalid
    #[error("SSH key error: {reason}")]
    KeyError { reason: String },

    /// Host key verification failed
    #[error("Host key verification failed: {reason}")]
    HostKeyVerification { reason: String },

    /// Channel creation failed
    #[error("SSH channel creation failed: {reason}")]
    ChannelCreation { reason: String },

    /// Command execution failed
    #[error("SSH command execution failed: {reason}")]
    CommandExecution { reason: String },

    /// Binary upload failed
    #[error("Binary upload failed: {reason}")]
    BinaryUpload { reason: String },
}

/// Local transport errors
#[derive(Error, Debug)]
pub enum LocalError {
    /// Binary not found
    #[error("Local binary not found: {path}")]
    BinaryNotFound { path: String },

    /// Binary not executable
    #[error("Local binary not executable: {path}")]
    BinaryNotExecutable { path: String },

    /// Process spawn failed
    #[error("Process spawn failed: {reason}")]
    ProcessSpawn { reason: String },

    /// Process communication failed
    #[error("Process communication failed: {reason}")]
    ProcessCommunication { reason: String },
}

/// TCP transport errors
#[derive(Error, Debug)]
pub enum TcpError {
    /// Connection timeout
    #[error("TCP connection timeout: {timeout_ms}ms")]
    ConnectionTimeout { timeout_ms: u64 },

    /// Connection refused
    #[error("TCP connection refused: {address}")]
    ConnectionRefused { address: String },

    /// TLS/SSL errors
    #[error("TLS error: {reason}")]
    TlsError { reason: String },
}

/// WSL transport errors
#[derive(Error, Debug)]
pub enum WslError {
    /// WSL not available
    #[error("WSL not available on this system")]
    NotAvailable,

    /// Distribution not found
    #[error("WSL distribution not found: {distribution}")]
    DistributionNotFound { distribution: String },

    /// Command execution failed
    #[error("WSL command execution failed: {reason}")]
    CommandExecution { reason: String },
}

/// Protocol communication errors
#[derive(Error, Debug)]
pub enum ProtocolError {
    /// Serialization failed
    #[error("Message serialization failed: {reason}")]
    Serialization { reason: String },

    /// Deserialization failed
    #[error("Message deserialization failed: {reason}")]
    Deserialization { reason: String },

    /// Protocol version mismatch
    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },

    /// Invalid message format
    #[error("Invalid message format: {reason}")]
    InvalidFormat { reason: String },

    /// Channel closed unexpectedly
    #[error("Protocol channel closed unexpectedly")]
    ChannelClosed,

    /// Timeout occurred
    #[error("Protocol timeout after {seconds} seconds")]
    Timeout { seconds: u64 },

    /// Buffer overflow
    #[error("Protocol buffer overflow: message too large ({size} bytes)")]
    BufferOverflow { size: usize },
}

/// Session management errors
#[derive(Error, Debug)]
pub enum SessionError {
    /// Session not found
    #[error("Session not found: {session_id}")]
    NotFound { session_id: String },

    /// Session already exists
    #[error("Session already exists: {session_id}")]
    AlreadyExists { session_id: String },

    /// Maximum sessions reached
    #[error("Maximum sessions reached: {limit}")]
    MaxSessionsReached { limit: u32 },

    /// Session in invalid state
    #[error("Session in invalid state: {current_state}, expected: {expected_state}")]
    InvalidState {
        current_state: String,
        expected_state: String,
    },

    /// Session expired
    #[error("Session expired: {session_id}")]
    Expired { session_id: String },

    /// Connection pooling error
    #[error("Connection pooling error: {reason}")]
    PoolingError { reason: String },
}

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Invalid configuration value
    #[error("Invalid configuration value for '{key}': {reason}")]
    InvalidValue { key: String, reason: String },

    /// Missing required configuration
    #[error("Missing required configuration: {key}")]
    MissingRequired { key: String },

    /// Configuration file error
    #[error("Configuration file error: {reason}")]
    FileError { reason: String },

    /// Profile not found
    #[error("Configuration profile not found: {profile}")]
    ProfileNotFound { profile: String },

    /// Validation failed
    #[error("Configuration validation failed: {reason}")]
    ValidationFailed { reason: String },
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

/// Authentication/authorization errors
#[derive(Error, Debug)]
pub enum AuthError {
    /// Invalid credentials
    #[error("Invalid credentials: {reason}")]
    InvalidCredentials { reason: String },

    /// Permission denied
    #[error("Permission denied: {operation}")]
    PermissionDenied { operation: String },

    /// Token expired
    #[error("Authentication token expired")]
    TokenExpired,

    /// Authentication method not supported
    #[error("Authentication method not supported: {method}")]
    MethodNotSupported { method: String },
}

/// Daemon-specific errors
#[derive(Error, Debug)]
pub enum DaemonError {
    /// Daemon not running
    #[error("Daemon not running")]
    NotRunning,

    /// Daemon already running
    #[error("Daemon already running with PID: {pid}")]
    AlreadyRunning { pid: u32 },

    /// Socket error
    #[error("Daemon socket error: {reason}")]
    SocketError { reason: String },

    /// Client limit reached
    #[error("Maximum client connections reached: {limit}")]
    ClientLimitReached { limit: u32 },

    /// Shutdown in progress
    #[error("Daemon is shutting down")]
    ShuttingDown,
}

// Convenience constructors for common error cases
impl YuhaError {
    pub fn protocol<S: Into<String>>(message: S) -> Self {
        Self::Protocol(ProtocolError::InvalidFormat {
            reason: message.into(),
        })
    }

    pub fn config<S: Into<String>>(message: S) -> Self {
        Self::Config(ConfigError::ValidationFailed {
            reason: message.into(),
        })
    }

    pub fn session<S: Into<String>>(message: S) -> Self {
        Self::Session(SessionError::InvalidState {
            current_state: "unknown".to_string(),
            expected_state: message.into(),
        })
    }

    pub fn internal<S: Into<String>>(message: S) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// Check if this error is retriable
    pub fn is_retriable(&self) -> bool {
        match self {
            YuhaError::Transport(TransportError::ConnectionFailed { .. }) => true,
            YuhaError::Protocol(ProtocolError::Timeout { .. }) => true,
            YuhaError::Protocol(ProtocolError::ChannelClosed) => true,
            YuhaError::Io(_) => true,
            YuhaError::Daemon(DaemonError::SocketError { .. }) => true,
            _ => false,
        }
    }

    /// Get the error category for logging/metrics
    pub fn category(&self) -> &'static str {
        match self {
            YuhaError::Transport(_) => "transport",
            YuhaError::Protocol(_) => "protocol",
            YuhaError::Session(_) => "session",
            YuhaError::Config(_) => "config",
            YuhaError::Io(_) => "io",
            YuhaError::Clipboard(_) => "clipboard",
            YuhaError::Browser(_) => "browser",
            YuhaError::Auth(_) => "auth",
            YuhaError::Daemon(_) => "daemon",
            YuhaError::Internal { .. } => "internal",
        }
    }

    /// Get a user-friendly error message
    pub fn user_message(&self) -> String {
        match self {
            YuhaError::Transport(TransportError::ConnectionFailed { .. }) => {
                "Unable to connect to remote server. Please check your connection settings."
                    .to_string()
            }
            YuhaError::Config(ConfigError::FileError { .. }) => {
                "Configuration file error. Please check your configuration.".to_string()
            }
            YuhaError::Auth(AuthError::InvalidCredentials { .. }) => {
                "Invalid credentials. Please check your username and password.".to_string()
            }
            _ => self.to_string(),
        }
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
