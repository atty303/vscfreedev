//! Configuration management for yuha

use crate::error::{Result, YuhaError};
use crate::logging::LoggingConfig;
use crate::metrics::MetricsConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Main configuration structure for yuha
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct YuhaConfig {
    /// Client configuration
    pub client: ClientConfig,
    /// Remote server configuration
    pub remote: RemoteConfig,
    /// Network configuration
    pub network: NetworkConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Metrics configuration
    pub metrics: MetricsConfig,
    /// Connection profiles
    #[serde(default)]
    pub profiles: HashMap<String, ConnectionProfile>,
}

/// Client-side configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Default connection timeout in seconds
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,
    /// Maximum retry attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Default binary path for local execution
    pub default_binary_path: Option<PathBuf>,
    /// Auto-upload binary by default
    #[serde(default)]
    pub auto_upload_binary: bool,
    /// Working directory for remote execution
    pub working_dir: Option<PathBuf>,
}

/// Remote server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    /// Default TCP port for listening
    #[serde(default = "default_tcp_port")]
    pub default_port: u16,
    /// Long polling timeout in seconds
    #[serde(default = "default_polling_timeout")]
    pub polling_timeout: u64,
    /// Polling interval in milliseconds
    #[serde(default = "default_polling_interval")]
    pub polling_interval: u64,
    /// Buffer size for message channels
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    /// Temporary file paths
    pub temp_files: TempFileConfig,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Default SSH port
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    /// Default bind address
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
    /// Port forwarding configuration
    pub port_forward: PortForwardConfig,
    /// Socket timeouts
    pub timeouts: TimeoutConfig,
}

/// Port forwarding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForwardConfig {
    /// Buffer size for port forwarding
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    /// Maximum concurrent port forwards
    #[serde(default = "default_max_port_forwards")]
    pub max_concurrent: u32,
    /// Connection timeout for port forwarding
    #[serde(default = "default_port_forward_timeout")]
    pub timeout: u64,
}

/// Timeout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// Connect timeout in seconds
    #[serde(default = "default_connect_timeout")]
    pub connect: u64,
    /// Read timeout in seconds
    #[serde(default = "default_read_timeout")]
    pub read: u64,
    /// Write timeout in seconds
    #[serde(default = "default_write_timeout")]
    pub write: u64,
}

/// Temporary file configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempFileConfig {
    /// Startup log file path
    #[serde(default = "default_startup_file")]
    pub startup_file: PathBuf,
    /// Panic log file path
    #[serde(default = "default_panic_file")]
    pub panic_file: PathBuf,
    /// Stderr log file path
    #[serde(default = "default_stderr_file")]
    pub stderr_file: PathBuf,
}

/// Connection profile for reusable connection settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    /// Profile name
    pub name: String,
    /// SSH connection details
    pub ssh: Option<SshConfig>,
    /// Local execution settings
    pub local: Option<LocalConfig>,
    /// Environment variables
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
    /// Custom settings that override defaults
    #[serde(flatten)]
    pub overrides: HashMap<String, toml::Value>,
}

/// SSH connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    /// Hostname or IP address
    pub host: String,
    /// SSH port
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    /// Username
    pub username: String,
    /// Password (not recommended, use key authentication)
    pub password: Option<String>,
    /// Private key file path
    pub key_path: Option<PathBuf>,
    /// Auto-upload binary
    #[serde(default)]
    pub auto_upload_binary: bool,
}

/// Local execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    /// Binary path
    pub binary_path: PathBuf,
    /// Command line arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory
    pub working_dir: Option<PathBuf>,
}

// Default value functions
fn default_connection_timeout() -> u64 {
    30
}
fn default_max_retries() -> u32 {
    3
}
fn default_tcp_port() -> u16 {
    9999
}
fn default_polling_timeout() -> u64 {
    5
}
fn default_polling_interval() -> u64 {
    10
}
fn default_buffer_size() -> usize {
    4096
}
fn default_ssh_port() -> u16 {
    22
}
fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}
fn default_max_port_forwards() -> u32 {
    100
}
fn default_port_forward_timeout() -> u64 {
    60
}
fn default_connect_timeout() -> u64 {
    10
}
fn default_read_timeout() -> u64 {
    30
}
fn default_write_timeout() -> u64 {
    30
}
fn default_startup_file() -> PathBuf {
    PathBuf::from("/tmp/yuha_startup.txt")
}
fn default_panic_file() -> PathBuf {
    PathBuf::from("/tmp/yuha_panic.txt")
}
fn default_stderr_file() -> PathBuf {
    PathBuf::from("/tmp/yuha_stderr.log")
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connection_timeout: default_connection_timeout(),
            max_retries: default_max_retries(),
            default_binary_path: None,
            auto_upload_binary: false,
            working_dir: None,
        }
    }
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            default_port: default_tcp_port(),
            polling_timeout: default_polling_timeout(),
            polling_interval: default_polling_interval(),
            buffer_size: default_buffer_size(),
            temp_files: TempFileConfig::default(),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            ssh_port: default_ssh_port(),
            bind_address: default_bind_address(),
            port_forward: PortForwardConfig::default(),
            timeouts: TimeoutConfig::default(),
        }
    }
}

impl Default for PortForwardConfig {
    fn default() -> Self {
        Self {
            buffer_size: default_buffer_size(),
            max_concurrent: default_max_port_forwards(),
            timeout: default_port_forward_timeout(),
        }
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect: default_connect_timeout(),
            read: default_read_timeout(),
            write: default_write_timeout(),
        }
    }
}

impl Default for TempFileConfig {
    fn default() -> Self {
        Self {
            startup_file: default_startup_file(),
            panic_file: default_panic_file(),
            stderr_file: default_stderr_file(),
        }
    }
}

impl YuhaConfig {
    /// Load configuration from file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        debug!("Loading configuration from: {}", path.display());

        let contents = std::fs::read_to_string(path)
            .map_err(|e| YuhaError::config(format!("Failed to read config file: {}", e)))?;

        let config: YuhaConfig = toml::from_str(&contents)
            .map_err(|e| YuhaError::config(format!("Failed to parse config file: {}", e)))?;

        info!("Configuration loaded successfully from: {}", path.display());
        Ok(config)
    }

    /// Save configuration to file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        debug!("Saving configuration to: {}", path.display());

        let contents = toml::to_string_pretty(self)
            .map_err(|e| YuhaError::config(format!("Failed to serialize config: {}", e)))?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                YuhaError::config(format!("Failed to create config directory: {}", e))
            })?;
        }

        std::fs::write(path, contents)
            .map_err(|e| YuhaError::config(format!("Failed to write config file: {}", e)))?;

        info!("Configuration saved successfully to: {}", path.display());
        Ok(())
    }

    /// Load configuration with fallback paths
    pub fn load_with_fallback() -> Self {
        let mut config_paths = vec![
            // Project-specific config
            PathBuf::from("yuha.toml"),
            PathBuf::from(".yuha.toml"),
        ];

        // User-specific config
        if let Some(config_dir) = dirs::config_dir() {
            config_paths.push(config_dir.join("yuha").join("config.toml"));
        }
        if let Some(home_dir) = dirs::home_dir() {
            config_paths.push(home_dir.join(".yuha.toml"));
        }

        // System-wide config
        config_paths.push(PathBuf::from("/etc/yuha/config.toml"));

        for config_path in config_paths {
            if config_path.exists() {
                match Self::load_from_file(&config_path) {
                    Ok(config) => {
                        info!("Using configuration from: {}", config_path.display());
                        return config;
                    }
                    Err(e) => {
                        warn!(
                            "Failed to load config from {}: {}",
                            config_path.display(),
                            e
                        );
                    }
                }
            }
        }

        info!("No configuration file found, using defaults");
        Self::default()
    }

    /// Get a connection profile by name
    pub fn get_profile(&self, name: &str) -> Option<&ConnectionProfile> {
        self.profiles.get(name)
    }

    /// Add or update a connection profile
    pub fn set_profile(&mut self, profile: ConnectionProfile) {
        self.profiles.insert(profile.name.clone(), profile);
    }

    /// Remove a connection profile
    pub fn remove_profile(&mut self, name: &str) -> Option<ConnectionProfile> {
        self.profiles.remove(name)
    }

    /// Get timeout as Duration
    pub fn connection_timeout(&self) -> Duration {
        Duration::from_secs(self.client.connection_timeout)
    }

    /// Get polling timeout as Duration
    pub fn polling_timeout(&self) -> Duration {
        Duration::from_secs(self.remote.polling_timeout)
    }

    /// Get polling interval as Duration
    pub fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.remote.polling_interval)
    }

    /// Merge configuration with environment variables
    pub fn merge_with_env(&mut self) {
        // Log level from RUST_LOG
        if let Ok(rust_log) = std::env::var("RUST_LOG") {
            if let Ok(level) = rust_log.parse() {
                self.logging.level = level;
            }
        }

        // Override specific settings from environment
        if let Ok(port) = std::env::var("YUHA_TCP_PORT") {
            if let Ok(port) = port.parse::<u16>() {
                self.remote.default_port = port;
            }
        }

        if let Ok(timeout) = std::env::var("YUHA_TIMEOUT") {
            if let Ok(timeout) = timeout.parse::<u64>() {
                self.client.connection_timeout = timeout;
            }
        }

        if let Ok(binary_path) = std::env::var("YUHA_BINARY_PATH") {
            self.client.default_binary_path = Some(PathBuf::from(binary_path));
        }

        debug!("Configuration merged with environment variables");
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate ports are in valid range
        if self.network.ssh_port == 0 {
            return Err(YuhaError::config("SSH port cannot be 0"));
        }

        if self.remote.default_port == 0 {
            return Err(YuhaError::config("TCP port cannot be 0"));
        }

        // Validate timeouts are reasonable
        if self.client.connection_timeout == 0 {
            return Err(YuhaError::config(
                "Connection timeout must be greater than 0",
            ));
        }

        if self.remote.polling_timeout == 0 {
            return Err(YuhaError::config("Polling timeout must be greater than 0"));
        }

        // Validate buffer sizes
        if self.remote.buffer_size == 0 {
            return Err(YuhaError::config("Buffer size must be greater than 0"));
        }

        // Validate log level (LogLevel enum is already validated by its type)

        debug!("Configuration validation completed successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = YuhaConfig::default();
        assert_eq!(config.network.ssh_port, 22);
        assert_eq!(config.remote.default_port, 9999);
        assert_eq!(config.client.connection_timeout, 30);
    }

    #[test]
    fn test_config_serialization() {
        let config = YuhaConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: YuhaConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(config.network.ssh_port, deserialized.network.ssh_port);
        assert_eq!(config.remote.default_port, deserialized.remote.default_port);
    }

    #[test]
    fn test_config_file_operations() {
        let config = YuhaConfig::default();
        let temp_file = NamedTempFile::new().unwrap();

        // Save config
        config.save_to_file(temp_file.path()).unwrap();

        // Load config
        let loaded_config = YuhaConfig::load_from_file(temp_file.path()).unwrap();
        assert_eq!(config.network.ssh_port, loaded_config.network.ssh_port);
    }

    #[test]
    fn test_config_validation() {
        let mut config = YuhaConfig::default();
        assert!(config.validate().is_ok());

        // Test invalid port
        config.network.ssh_port = 0;
        assert!(config.validate().is_err());

        // Test invalid timeout
        config.network.ssh_port = 22;
        config.client.connection_timeout = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_profile_management() {
        let mut config = YuhaConfig::default();

        let profile = ConnectionProfile {
            name: "test".to_string(),
            ssh: Some(SshConfig {
                host: "example.com".to_string(),
                port: 22,
                username: "user".to_string(),
                password: None,
                key_path: None,
                auto_upload_binary: false,
            }),
            local: None,
            env_vars: HashMap::new(),
            overrides: HashMap::new(),
        };

        config.set_profile(profile.clone());
        assert!(config.get_profile("test").is_some());

        let removed = config.remove_profile("test");
        assert!(removed.is_some());
        assert!(config.get_profile("test").is_none());
    }
}
