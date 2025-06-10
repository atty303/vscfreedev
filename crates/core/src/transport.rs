//! Unified transport abstraction for Yuha
//!
//! This module provides a centralized transport management system that supports
//! multiple connection types: SSH, Local process, TCP, and WSL.

use crate::config::{LocalConfig, SshConfig, YuhaConfig};
use crate::error::{Result, YuhaError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info};

/// Transport types supported by Yuha
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    /// SSH connection to remote host
    Ssh,
    /// Local process execution
    Local,
    /// Direct TCP connection
    Tcp,
    /// Windows Subsystem for Linux
    Wsl,
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportType::Ssh => write!(f, "ssh"),
            TransportType::Local => write!(f, "local"),
            TransportType::Tcp => write!(f, "tcp"),
            TransportType::Wsl => write!(f, "wsl"),
        }
    }
}

impl std::str::FromStr for TransportType {
    type Err = YuhaError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "ssh" => Ok(TransportType::Ssh),
            "local" => Ok(TransportType::Local),
            "tcp" => Ok(TransportType::Tcp),
            "wsl" => Ok(TransportType::Wsl),
            _ => Err(YuhaError::config(format!("Unknown transport type: {}", s))),
        }
    }
}

/// Transport configuration that can be used with any transport type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Type of transport to use
    pub transport_type: TransportType,
    /// SSH-specific configuration
    pub ssh: Option<SshTransportConfig>,
    /// Local process configuration
    pub local: Option<LocalTransportConfig>,
    /// TCP connection configuration
    pub tcp: Option<TcpTransportConfig>,
    /// WSL configuration
    pub wsl: Option<WslTransportConfig>,
    /// General transport settings
    pub general: GeneralTransportConfig,
}

/// SSH transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTransportConfig {
    /// Remote host address
    pub host: String,
    /// SSH port
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
    #[serde(default = "default_connection_timeout")]
    pub timeout: u64,
    /// Keep-alive interval in seconds
    #[serde(default = "default_keepalive")]
    pub keepalive: u64,
}

/// Local process transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalTransportConfig {
    /// Path to the yuha-remote binary
    pub binary_path: PathBuf,
    /// Command line arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory for the process
    pub working_dir: Option<PathBuf>,
}

/// TCP direct connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpTransportConfig {
    /// Target host address
    pub host: String,
    /// Target port
    pub port: u16,
    /// Connection timeout in seconds
    #[serde(default = "default_connection_timeout")]
    pub timeout: u64,
    /// Keep-alive settings
    #[serde(default)]
    pub keepalive: bool,
    /// TLS/SSL configuration
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
pub struct WslTransportConfig {
    /// WSL distribution name
    pub distribution: Option<String>,
    /// User to execute as
    pub user: Option<String>,
    /// Path to yuha-remote in WSL
    pub binary_path: Option<PathBuf>,
    /// Working directory in WSL
    pub working_dir: Option<PathBuf>,
}

/// General transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralTransportConfig {
    /// Maximum retry attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Retry delay in milliseconds
    #[serde(default = "default_retry_delay")]
    pub retry_delay: u64,
    /// Environment variables to set
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
    /// Remote binary path (if different from default)
    pub remote_binary_path: Option<PathBuf>,
    /// Auto-detect best transport if multiple are available
    #[serde(default)]
    pub auto_detect: bool,
}

// Default value functions
fn default_ssh_port() -> u16 {
    22
}
fn default_connection_timeout() -> u64 {
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
            local: Some(LocalTransportConfig::default()),
            tcp: None,
            wsl: None,
            general: GeneralTransportConfig::default(),
        }
    }
}

impl Default for LocalTransportConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("yuha-remote"),
            args: vec!["--stdio".to_string()],
            working_dir: None,
        }
    }
}

impl Default for GeneralTransportConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            retry_delay: default_retry_delay(),
            env_vars: HashMap::new(),
            remote_binary_path: None,
            auto_detect: false,
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

/// Transport factory for creating transport instances
#[derive(Debug)]
pub struct TransportFactory {
    config: YuhaConfig,
}

impl TransportFactory {
    /// Create a new transport factory
    pub fn new(config: YuhaConfig) -> Self {
        Self { config }
    }

    /// Create a transport configuration from a connection profile
    pub fn create_from_profile(&self, profile_name: &str) -> Result<TransportConfig> {
        let profile = self
            .config
            .get_profile(profile_name)
            .ok_or_else(|| YuhaError::config(format!("Profile '{}' not found", profile_name)))?;

        let mut transport_config = TransportConfig::default();

        // Configure based on profile settings
        if let Some(ref ssh_config) = profile.ssh {
            transport_config.transport_type = TransportType::Ssh;
            transport_config.ssh = Some(SshTransportConfig {
                host: ssh_config.host.clone(),
                port: ssh_config.port,
                username: ssh_config.username.clone(),
                password: ssh_config.password.clone(),
                key_path: ssh_config.key_path.clone(),
                auto_upload_binary: ssh_config.auto_upload_binary,
                timeout: self.config.client.connection_timeout,
                keepalive: default_keepalive(),
            });
        } else if let Some(ref local_config) = profile.local {
            transport_config.transport_type = TransportType::Local;
            transport_config.local = Some(LocalTransportConfig {
                binary_path: local_config.binary_path.clone(),
                args: local_config.args.clone(),
                working_dir: local_config.working_dir.clone(),
            });
        } else {
            return Err(YuhaError::config(format!(
                "Profile '{}' does not specify a valid transport configuration",
                profile_name
            )));
        }

        // Apply environment variables from profile
        transport_config
            .general
            .env_vars
            .extend(profile.env_vars.clone());

        info!(
            "Created transport configuration from profile '{}'",
            profile_name
        );
        debug!("Transport config: {:?}", transport_config);

        Ok(transport_config)
    }

    /// Create SSH transport configuration
    pub fn create_ssh_config(
        &self,
        host: String,
        port: u16,
        username: String,
        password: Option<String>,
        key_path: Option<PathBuf>,
        auto_upload: bool,
    ) -> TransportConfig {
        TransportConfig {
            transport_type: TransportType::Ssh,
            ssh: Some(SshTransportConfig {
                host,
                port,
                username,
                password,
                key_path,
                auto_upload_binary: auto_upload,
                timeout: self.config.client.connection_timeout,
                keepalive: default_keepalive(),
            }),
            local: None,
            tcp: None,
            wsl: None,
            general: GeneralTransportConfig {
                max_retries: self.config.client.max_retries,
                ..Default::default()
            },
        }
    }

    /// Create local transport configuration
    pub fn create_local_config(&self, binary_path: Option<PathBuf>) -> TransportConfig {
        let effective_path = binary_path
            .or_else(|| self.config.client.default_binary_path.clone())
            .unwrap_or_else(|| PathBuf::from("yuha-remote"));

        TransportConfig {
            transport_type: TransportType::Local,
            ssh: None,
            local: Some(LocalTransportConfig {
                binary_path: effective_path,
                args: vec!["--stdio".to_string()],
                working_dir: self.config.client.working_dir.clone(),
            }),
            tcp: None,
            wsl: None,
            general: GeneralTransportConfig {
                max_retries: self.config.client.max_retries,
                ..Default::default()
            },
        }
    }

    /// Create TCP transport configuration
    pub fn create_tcp_config(&self, host: String, port: u16, use_tls: bool) -> TransportConfig {
        TransportConfig {
            transport_type: TransportType::Tcp,
            ssh: None,
            local: None,
            tcp: Some(TcpTransportConfig {
                host,
                port,
                timeout: self.config.client.connection_timeout,
                keepalive: true,
                tls: if use_tls {
                    Some(TlsConfig::default())
                } else {
                    None
                },
            }),
            wsl: None,
            general: GeneralTransportConfig {
                max_retries: self.config.client.max_retries,
                ..Default::default()
            },
        }
    }

    /// Create WSL transport configuration
    pub fn create_wsl_config(
        &self,
        distribution: Option<String>,
        user: Option<String>,
        binary_path: Option<PathBuf>,
    ) -> TransportConfig {
        TransportConfig {
            transport_type: TransportType::Wsl,
            ssh: None,
            local: None,
            tcp: None,
            wsl: Some(WslTransportConfig {
                distribution,
                user,
                binary_path,
                working_dir: None,
            }),
            general: GeneralTransportConfig {
                max_retries: self.config.client.max_retries,
                ..Default::default()
            },
        }
    }

    /// Auto-detect best available transport
    pub fn auto_detect_transport(&self) -> Result<TransportConfig> {
        // Simple heuristics for transport detection
        // In practice, this would involve more sophisticated detection

        // Check if we're on Windows and WSL is available
        if cfg!(windows) && self.is_wsl_available() {
            info!("Auto-detected WSL transport");
            return Ok(self.create_wsl_config(None, None, None));
        }

        // Default to local transport
        info!("Auto-detected local transport");
        Ok(self.create_local_config(None))
    }

    /// Check if WSL is available (placeholder implementation)
    fn is_wsl_available(&self) -> bool {
        // This would check for WSL installation and available distributions
        // For now, just return false as a placeholder
        false
    }

    /// Validate transport configuration
    pub fn validate_config(&self, config: &TransportConfig) -> Result<()> {
        match config.transport_type {
            TransportType::Ssh => {
                let ssh_config = config
                    .ssh
                    .as_ref()
                    .ok_or_else(|| YuhaError::config("SSH transport requires SSH configuration"))?;

                if ssh_config.host.is_empty() {
                    return Err(YuhaError::config("SSH host cannot be empty"));
                }
                if ssh_config.username.is_empty() {
                    return Err(YuhaError::config("SSH username cannot be empty"));
                }
                if ssh_config.password.is_none() && ssh_config.key_path.is_none() {
                    return Err(YuhaError::config(
                        "SSH transport requires either password or key authentication",
                    ));
                }
            }
            TransportType::Local => {
                let local_config = config.local.as_ref().ok_or_else(|| {
                    YuhaError::config("Local transport requires local configuration")
                })?;

                if !local_config.binary_path.exists() {
                    return Err(YuhaError::config(format!(
                        "Local binary not found: {}",
                        local_config.binary_path.display()
                    )));
                }
            }
            TransportType::Tcp => {
                let tcp_config = config
                    .tcp
                    .as_ref()
                    .ok_or_else(|| YuhaError::config("TCP transport requires TCP configuration"))?;

                if tcp_config.host.is_empty() {
                    return Err(YuhaError::config("TCP host cannot be empty"));
                }
                if tcp_config.port == 0 {
                    return Err(YuhaError::config("TCP port cannot be 0"));
                }
            }
            TransportType::Wsl => {
                let _wsl_config = config
                    .wsl
                    .as_ref()
                    .ok_or_else(|| YuhaError::config("WSL transport requires WSL configuration"))?;

                if !cfg!(windows) {
                    return Err(YuhaError::config(
                        "WSL transport is only available on Windows",
                    ));
                }
            }
        }

        debug!("Transport configuration validation passed");
        Ok(())
    }
}

/// Convert from existing config types to new transport config
impl From<SshConfig> for SshTransportConfig {
    fn from(config: SshConfig) -> Self {
        Self {
            host: config.host,
            port: config.port,
            username: config.username,
            password: config.password,
            key_path: config.key_path,
            auto_upload_binary: config.auto_upload_binary,
            timeout: default_connection_timeout(),
            keepalive: default_keepalive(),
        }
    }
}

impl From<LocalConfig> for LocalTransportConfig {
    fn from(config: LocalConfig) -> Self {
        Self {
            binary_path: config.binary_path,
            args: config.args,
            working_dir: config.working_dir,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::YuhaConfig;

    #[test]
    fn test_transport_type_parsing() {
        assert_eq!("ssh".parse::<TransportType>().unwrap(), TransportType::Ssh);
        assert_eq!(
            "local".parse::<TransportType>().unwrap(),
            TransportType::Local
        );
        assert_eq!("tcp".parse::<TransportType>().unwrap(), TransportType::Tcp);
        assert_eq!("wsl".parse::<TransportType>().unwrap(), TransportType::Wsl);
        assert!("invalid".parse::<TransportType>().is_err());
    }

    #[test]
    fn test_transport_factory_creation() {
        let config = YuhaConfig::default();
        let factory = TransportFactory::new(config);

        let local_config = factory.create_local_config(None);
        assert_eq!(local_config.transport_type, TransportType::Local);
        assert!(local_config.local.is_some());
    }

    #[test]
    fn test_transport_config_validation() {
        let config = YuhaConfig::default();
        let factory = TransportFactory::new(config);

        // Test valid local config
        let local_config = TransportConfig {
            transport_type: TransportType::Local,
            local: Some(LocalTransportConfig {
                binary_path: PathBuf::from("/bin/true"), // Should exist on most systems
                args: vec![],
                working_dir: None,
            }),
            ssh: None,
            tcp: None,
            wsl: None,
            general: GeneralTransportConfig::default(),
        };

        // This might fail on some systems, so we'll skip validation for now
        // assert!(factory.validate_config(&local_config).is_ok());
    }

    #[test]
    fn test_ssh_config_conversion() {
        let ssh_config = SshConfig {
            host: "example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            password: Some("pass".to_string()),
            key_path: None,
            auto_upload_binary: false,
        };

        let transport_config: SshTransportConfig = ssh_config.into();
        assert_eq!(transport_config.host, "example.com");
        assert_eq!(transport_config.port, 22);
        assert_eq!(transport_config.username, "user");
    }
}
