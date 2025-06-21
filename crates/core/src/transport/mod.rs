//! Simplified and unified transport abstraction
//!
//! This module provides a clean, simplified transport system that consolidates
//! the various transport configurations and implementations.

use crate::error::{Result, TransportError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::io::{AsyncRead, AsyncWrite};

pub mod builder;
pub mod factory;
pub mod types;

// Re-export commonly used types
pub use builder::TransportBuilder;
pub use factory::TransportFactory;
pub use types::*;

/// Trait that combines AsyncRead + AsyncWrite for transport streams
pub trait TransportStream: AsyncRead + AsyncWrite + Unpin + Send {}

// Blanket implementation for any type that implements all required traits
impl<T> TransportStream for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

/// Unified transport trait for all connection types
#[async_trait]
pub trait Transport: Send + Sync {
    /// Connect to the remote server and return a bidirectional stream
    async fn connect(&self) -> Result<Box<dyn TransportStream>>;

    /// Get the transport type name
    fn transport_type(&self) -> TransportType;

    /// Get a human-readable description
    fn description(&self) -> String;

    /// Check if the transport is available on this platform
    fn is_available(&self) -> bool;

    /// Get connection metadata
    fn metadata(&self) -> TransportMetadata;
}

/// Transport configuration that unifies all transport types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// The type of transport to use
    pub transport_type: TransportType,
    /// SSH configuration (if using SSH transport)
    pub ssh: Option<SshConfig>,
    /// Local process configuration (if using local transport)
    pub local: Option<LocalConfig>,
    /// TCP configuration (if using TCP transport)
    pub tcp: Option<TcpConfig>,
    /// WSL configuration (if using WSL transport)
    pub wsl: Option<WslConfig>,
    /// General configuration that applies to all transports
    pub general: GeneralConfig,
}

/// SSH transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    /// Remote host address
    pub host: String,
    /// SSH port (default: 22)
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    /// Username for authentication
    pub username: String,
    /// Password for authentication (not recommended)
    pub password: Option<String>,
    /// Private key file path
    pub key_path: Option<PathBuf>,
    /// Auto-upload binary if not present
    #[serde(default)]
    pub auto_upload_binary: bool,
    /// Connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// Keep-alive interval in seconds
    #[serde(default = "default_keepalive")]
    pub keepalive: u64,
}

/// Local process transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    /// Path to the yuha-remote binary
    pub binary_path: PathBuf,
    /// Command line arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory for the process
    pub working_dir: Option<PathBuf>,
}

/// TCP transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpConfig {
    /// Target host address
    pub host: String,
    /// Target port
    pub port: u16,
    /// Connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// TLS configuration
    pub tls: Option<TlsConfig>,
}

/// TLS configuration for TCP transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS
    #[serde(default)]
    pub enabled: bool,
    /// Verify server certificate
    #[serde(default = "default_tls_verify")]
    pub verify_cert: bool,
    /// Client certificate path
    pub client_cert: Option<PathBuf>,
    /// Client private key path
    pub client_key: Option<PathBuf>,
    /// CA certificate path
    pub ca_cert: Option<PathBuf>,
}

/// WSL transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WslConfig {
    /// WSL distribution name
    pub distribution: Option<String>,
    /// User to execute as
    pub user: Option<String>,
    /// Path to yuha-remote in WSL
    pub binary_path: Option<PathBuf>,
    /// Working directory in WSL
    pub working_dir: Option<PathBuf>,
}

/// General configuration that applies to all transports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Maximum retry attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Retry delay in milliseconds
    #[serde(default = "default_retry_delay")]
    pub retry_delay: u64,
    /// Environment variables to set
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
    /// Remote binary path override
    pub remote_binary_path: Option<PathBuf>,
}

/// Transport metadata for introspection
#[derive(Debug, Clone)]
pub struct TransportMetadata {
    pub transport_type: TransportType,
    pub connection_string: String,
    pub is_secure: bool,
    pub supports_auto_upload: bool,
    pub platform_specific: bool,
}

// Default value functions
fn default_ssh_port() -> u16 {
    22
}
fn default_timeout() -> u64 {
    30
}
fn default_keepalive() -> u64 {
    60
}
fn default_max_retries() -> u32 {
    3
}
fn default_retry_delay() -> u64 {
    1000
}
fn default_tls_verify() -> bool {
    true
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            transport_type: TransportType::Local,
            ssh: None,
            local: Some(LocalConfig::default()),
            tcp: None,
            wsl: None,
            general: GeneralConfig::default(),
        }
    }
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("yuha-remote"),
            args: vec!["--stdio".to_string()],
            working_dir: None,
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            retry_delay: default_retry_delay(),
            env_vars: HashMap::new(),
            remote_binary_path: None,
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            verify_cert: default_tls_verify(),
            client_cert: None,
            client_key: None,
            ca_cert: None,
        }
    }
}

impl TransportConfig {
    /// Generate a connection key for identifying similar connections
    pub fn connection_key(&self) -> String {
        match self.transport_type {
            TransportType::Ssh => {
                if let Some(ssh) = &self.ssh {
                    format!("ssh://{}@{}:{}", ssh.username, ssh.host, ssh.port)
                } else {
                    "ssh://unknown".to_string()
                }
            }
            TransportType::Local => {
                if let Some(local) = &self.local {
                    format!("local://{}", local.binary_path.display())
                } else {
                    "local://unknown".to_string()
                }
            }
            TransportType::Tcp => {
                if let Some(tcp) = &self.tcp {
                    let scheme = if tcp.tls.as_ref().map(|t| t.enabled).unwrap_or(false) {
                        "tcps"
                    } else {
                        "tcp"
                    };
                    format!("{}://{}:{}", scheme, tcp.host, tcp.port)
                } else {
                    "tcp://unknown".to_string()
                }
            }
            TransportType::Wsl => {
                if let Some(wsl) = &self.wsl {
                    format!(
                        "wsl://{}@{}",
                        wsl.user.as_deref().unwrap_or("default"),
                        wsl.distribution.as_deref().unwrap_or("default")
                    )
                } else {
                    "wsl://default".to_string()
                }
            }
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        match self.transport_type {
            TransportType::Ssh => {
                let ssh = self
                    .ssh
                    .as_ref()
                    .ok_or_else(|| TransportError::ConfigurationError {
                        reason: "SSH transport requires SSH configuration".to_string(),
                    })?;

                if ssh.host.is_empty() {
                    return Err(TransportError::ConfigurationError {
                        reason: "SSH host cannot be empty".to_string(),
                    }
                    .into());
                }

                if ssh.username.is_empty() {
                    return Err(TransportError::ConfigurationError {
                        reason: "SSH username cannot be empty".to_string(),
                    }
                    .into());
                }

                if ssh.password.is_none() && ssh.key_path.is_none() {
                    return Err(TransportError::ConfigurationError {
                        reason: "SSH transport requires either password or key authentication"
                            .to_string(),
                    }
                    .into());
                }
            }
            TransportType::Local => {
                let local =
                    self.local
                        .as_ref()
                        .ok_or_else(|| TransportError::ConfigurationError {
                            reason: "Local transport requires local configuration".to_string(),
                        })?;

                if !local.binary_path.exists() {
                    return Err(TransportError::ConfigurationError {
                        reason: format!("Local binary not found: {}", local.binary_path.display()),
                    }
                    .into());
                }
            }
            TransportType::Tcp => {
                let tcp = self
                    .tcp
                    .as_ref()
                    .ok_or_else(|| TransportError::ConfigurationError {
                        reason: "TCP transport requires TCP configuration".to_string(),
                    })?;

                if tcp.host.is_empty() {
                    return Err(TransportError::ConfigurationError {
                        reason: "TCP host cannot be empty".to_string(),
                    }
                    .into());
                }

                if tcp.port == 0 {
                    return Err(TransportError::ConfigurationError {
                        reason: "TCP port cannot be 0".to_string(),
                    }
                    .into());
                }
            }
            TransportType::Wsl => {
                if !cfg!(windows) {
                    return Err(TransportError::NotAvailable {
                        transport_type: "WSL".to_string(),
                        reason: "WSL is only available on Windows".to_string(),
                    }
                    .into());
                }
            }
        }

        Ok(())
    }

    /// Check if the transport is available on this platform
    pub fn is_available(&self) -> bool {
        match self.transport_type {
            TransportType::Ssh | TransportType::Local | TransportType::Tcp => true,
            TransportType::Wsl => cfg!(windows),
        }
    }
}
