//! Transport abstraction for Yuha client
//!
//! This module provides an abstraction layer for different transport mechanisms
//! (SSH, Local process, WSL, etc.) used to communicate with the remote Yuha server.

use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::io::{AsyncRead, AsyncWrite};

pub mod local;
pub mod ssh;
pub mod tcp;
pub mod wsl;

pub use local::LocalTransport;
pub use ssh::SshTransport;
pub use tcp::TcpTransport;
pub use wsl::WslTransport;

/// Configuration for transport connection
#[derive(Debug, Clone, Default)]
pub struct TransportConfig {
    /// Path to the remote binary
    pub remote_binary_path: Option<PathBuf>,
    /// Whether to auto-upload the binary
    pub auto_upload_binary: bool,
    /// Additional environment variables for the remote process
    pub env_vars: Vec<(String, String)>,
    /// Working directory for the remote process
    pub working_dir: Option<PathBuf>,
}

/// Trait for transport implementations
#[async_trait]
pub trait Transport: Send + Sync {
    /// The stream type that implements AsyncRead + AsyncWrite
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    /// Connect to the remote server and return a bidirectional stream
    async fn connect(&self) -> Result<Self::Stream>;

    /// Get the name of this transport type
    fn name(&self) -> &'static str;
}

/// SSH transport configuration
#[derive(Debug, Clone)]
pub struct SshTransportConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub key_path: Option<PathBuf>,
}

/// Local transport configuration (for running the remote process locally)
#[derive(Debug, Clone)]
pub struct LocalTransportConfig {
    /// Path to the yuha-remote binary
    pub binary_path: PathBuf,
    /// Additional arguments to pass to the binary
    pub args: Vec<String>,
}

impl Default for LocalTransportConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("yuha-remote"),
            args: vec!["--stdio".to_string()],
        }
    }
}

/// WSL transport configuration (for future implementation)
#[derive(Debug, Clone)]
pub struct WslTransportConfig {
    pub distribution: Option<String>,
    pub user: Option<String>,
}
