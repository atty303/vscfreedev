//! Client-side library for yuha

pub mod simple_client;
pub mod simple_client_transport;
pub mod transport;

/// Path to the remote binary built by build.rs
pub const REMOTE_BINARY_PATH: &str = env!("YUHA_REMOTE_BINARY_PATH");

/// Error types for the client
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
}

// Re-export for backward compatibility
pub use transport::ssh::{MyHandler, SshChannelAdapter};

/// Public client interface (legacy)
pub mod client {
    use super::*;

    /// Get the path to the built remote binary
    pub fn get_remote_binary_path() -> &'static str {
        REMOTE_BINARY_PATH
    }
}
