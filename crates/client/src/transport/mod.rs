//! # Client Transport Layer
//!
//! This module provides client-specific transport implementations for connecting
//! to remote Yuha servers. It extends the core transport abstractions with
//! concrete implementations for various connection methods.
//!
//! ## Available Transports
//!
//! - **SSH Transport** (`ssh`): Connect via SSH with automatic binary management
//! - **Local Transport** (`local`): Spawn local yuha-remote process
//! - **TCP Transport** (`tcp`): Direct TCP connection to daemon
//! - **WSL Transport** (`wsl`): Windows Subsystem for Linux integration
//! - **Unix Transport** (`unix`): Unix domain sockets (Unix only)
//! - **Windows Transport** (`windows`): Named pipes (Windows only)
//!
//! ## Transport Selection
//!
//! Each transport is optimized for specific use cases:
//!
//! - Use **SSH** for remote servers with automatic setup
//! - Use **Local** for development and testing
//! - Use **TCP** for connecting to existing daemons
//! - Use **WSL** for Windows-to-WSL communication
//! - Use **Unix/Windows** for high-performance local IPC
//!
//! ## Configuration
//!
//! All transports support common configuration options through `TransportConfig`:
//!
//! ```rust,no_run
//! use yuha_client::transport::TransportConfig;
//!
//! let config = TransportConfig {
//!     auto_upload_binary: true,
//!     working_dir: Some("/tmp".into()),
//!     ..Default::default()
//! };
//! ```

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::io::{AsyncRead, AsyncWrite};

pub mod local;
pub mod shared;
pub mod ssh;
pub mod tcp;
pub mod wsl;

#[cfg(unix)]
pub mod unix;
#[cfg(windows)]
pub mod windows;

pub use local::LocalTransport;
pub use ssh::SshTransport;
pub use tcp::TcpTransport;
pub use wsl::WslTransport;

#[cfg(unix)]
pub use unix::UnixTransport;
#[cfg(windows)]
pub use windows::WindowsTransport;

/// Configuration for transport connection
#[derive(Debug, Clone, Default)]
pub struct TransportConfig {
    /// Path to the remote binary
    pub remote_binary_path: Option<PathBuf>,
    /// Whether to auto-upload the binary
    pub auto_upload_binary: bool,
    /// Additional environment variables for the remote process
    pub env_vars: HashMap<String, String>,
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
