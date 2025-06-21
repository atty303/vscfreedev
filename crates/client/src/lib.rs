//! # Yuha Client Library
//!
//! This crate provides client-side functionality for connecting to and communicating
//! with remote Yuha servers. It supports multiple connection methods and protocols.
//!
//! ## Key Components
//!
//! - **Simple Client**: Direct request-response communication with remote servers
//! - **Daemon Client**: Connection to local daemon for managing multiple sessions
//! - **Transport Layer**: Abstraction over SSH, TCP, and local connections
//! - **Protocol Handling**: Support for both simple and daemon communication protocols
//!
//! ## Connection Types
//!
//! - `SshTransport`: Connect to remote server via SSH with automatic binary upload
//! - `TcpTransport`: Direct TCP connection to running daemon
//! - `LocalTransport`: Spawn local process and communicate via stdin/stdout
//! - `WslTransport`: Windows Subsystem for Linux integration
//!
//! ## Usage Patterns
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use yuha_client::simple_client;
//!
//! // Connect to remote server via SSH
//! let client = simple_client::connect_ssh(
//!     "example.com", 22, "user", None, Some("/path/to/key".as_ref())
//! ).await?;
//!
//! // Use client for operations
//! client.get_clipboard().await?;
//! # Ok(())
//! # }
//! ```

pub mod constants;
pub mod daemon;
pub mod daemon_client;
pub mod daemon_protocol;
pub mod simple_client;
pub mod simple_client_transport;
pub mod transport;
pub mod transport_factory;

// Re-export main client type for convenience
pub use simple_client_transport::YuhaClient;

/// Path to the remote binary built by build.rs.
///
/// This constant is set at compile time by the build script and points to the
/// location of the yuha-remote binary that can be uploaded to remote servers.
///
/// # Usage
///
/// ```rust
/// use yuha_client::REMOTE_BINARY_PATH;
///
/// println!("Remote binary path: {}", REMOTE_BINARY_PATH);
/// ```
pub const REMOTE_BINARY_PATH: &str = env!("YUHA_REMOTE_BINARY_PATH");

/// Comprehensive error types for client operations.
///
/// This enum covers all possible error conditions that can occur during
/// client operations, from network connectivity issues to protocol errors.
///
/// # Error Categories
///
/// - **Connection Errors**: Network and transport-level failures
/// - **Authentication Errors**: SSH key and credential issues
/// - **Protocol Errors**: Message serialization and communication problems
/// - **Remote Execution Errors**: Issues running commands on remote servers
///
/// # Example
///
/// ```rust
/// use yuha_client::ClientError;
///
/// fn handle_error(error: ClientError) {
///     match error {
///         ClientError::Connection(msg) => {
///             eprintln!("Connection failed: {}", msg);
///         }
///         ClientError::Ssh(ssh_err) => {
///             eprintln!("SSH error: {}", ssh_err);
///         }
///         _ => {
///             eprintln!("Other error: {}", error);
///         }
///     }
/// }
/// ```
#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("SSH error: {0}")]
    Ssh(#[from] russh::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Remote execution error: {0}")]
    RemoteExecution(String),

    #[error("Channel error: {0}")]
    Channel(String),

    #[error("Key error: {0}")]
    Key(#[from] russh_keys::Error),

    #[error("Binary transfer error: {0}")]
    BinaryTransfer(String),

    #[error("Daemon error: {message} (code: {code:?})")]
    DaemonError {
        code: crate::daemon_protocol::ErrorCode,
        message: String,
    },
}

// Re-export commonly used transport types
pub use transport::ssh::{MyHandler, SshChannelAdapter};

/// Utility functions for client operations
pub mod client {
    use super::*;

    /// Get the path to the built remote binary
    pub fn get_remote_binary_path() -> &'static str {
        REMOTE_BINARY_PATH
    }
}
