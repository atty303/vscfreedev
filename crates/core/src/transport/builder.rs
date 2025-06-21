//! Builder pattern implementation for transport configuration

use super::{
    GeneralConfig, LocalConfig, SshConfig, TcpConfig, TlsConfig, TransportConfig, TransportType,
    WslConfig,
};
use crate::error::Result;
use std::path::PathBuf;

/// Builder for creating transport configurations using the builder pattern
pub struct TransportBuilder {
    config: TransportConfig,
}

impl TransportBuilder {
    /// Create a new transport builder with default settings
    pub fn new() -> Self {
        Self {
            config: TransportConfig::default(),
        }
    }

    /// Build an SSH transport configuration
    pub fn ssh() -> SshTransportBuilder {
        SshTransportBuilder::new()
    }

    /// Build a local transport configuration
    pub fn local() -> LocalTransportBuilder {
        LocalTransportBuilder::new()
    }

    /// Build a TCP transport configuration
    pub fn tcp() -> TcpTransportBuilder {
        TcpTransportBuilder::new()
    }

    /// Build a WSL transport configuration
    pub fn wsl() -> WslTransportBuilder {
        WslTransportBuilder::new()
    }

    /// Set general configuration
    pub fn with_general(mut self, general: GeneralConfig) -> Self {
        self.config.general = general;
        self
    }

    /// Add environment variable
    pub fn with_env_var<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.config
            .general
            .env_vars
            .insert(key.into(), value.into());
        self
    }

    /// Set retry configuration
    pub fn with_retries(mut self, max_retries: u32, retry_delay: u64) -> Self {
        self.config.general.max_retries = max_retries;
        self.config.general.retry_delay = retry_delay;
        self
    }

    /// Build the final configuration
    pub fn build(self) -> Result<TransportConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

impl Default for TransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// SSH transport builder
pub struct SshTransportBuilder {
    config: SshConfig,
    general: GeneralConfig,
}

impl SshTransportBuilder {
    fn new() -> Self {
        Self {
            config: SshConfig {
                host: String::new(),
                port: 22,
                username: String::new(),
                password: None,
                key_path: None,
                auto_upload_binary: false,
                timeout: 30,
                keepalive: 60,
            },
            general: GeneralConfig::default(),
        }
    }

    /// Set the SSH host
    pub fn host<S: Into<String>>(mut self, host: S) -> Self {
        self.config.host = host.into();
        self
    }

    /// Set the SSH port
    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    /// Set the username
    pub fn username<S: Into<String>>(mut self, username: S) -> Self {
        self.config.username = username.into();
        self
    }

    /// Set password authentication
    pub fn password<S: Into<String>>(mut self, password: S) -> Self {
        self.config.password = Some(password.into());
        self
    }

    /// Set key-based authentication
    pub fn key_file<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.config.key_path = Some(path.into());
        self
    }

    /// Enable automatic binary upload
    pub fn auto_upload_binary(mut self) -> Self {
        self.config.auto_upload_binary = true;
        self
    }

    /// Set connection timeout
    pub fn timeout(mut self, seconds: u64) -> Self {
        self.config.timeout = seconds;
        self
    }

    /// Set keepalive interval
    pub fn keepalive(mut self, seconds: u64) -> Self {
        self.config.keepalive = seconds;
        self
    }

    /// Add environment variable
    pub fn with_env_var<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.general.env_vars.insert(key.into(), value.into());
        self
    }

    /// Build the SSH transport configuration
    pub fn build(self) -> Result<TransportConfig> {
        let config = TransportConfig {
            transport_type: TransportType::Ssh,
            ssh: Some(self.config),
            local: None,
            tcp: None,
            wsl: None,
            general: self.general,
        };
        config.validate()?;
        Ok(config)
    }
}

/// Local transport builder
pub struct LocalTransportBuilder {
    config: LocalConfig,
    general: GeneralConfig,
}

impl LocalTransportBuilder {
    fn new() -> Self {
        Self {
            config: LocalConfig::default(),
            general: GeneralConfig::default(),
        }
    }

    /// Set the binary path
    pub fn binary_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.config.binary_path = path.into();
        self
    }

    /// Add command line argument
    pub fn arg<S: Into<String>>(mut self, arg: S) -> Self {
        self.config.args.push(arg.into());
        self
    }

    /// Set command line arguments
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.config.args = args.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Set working directory
    pub fn working_dir<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.config.working_dir = Some(path.into());
        self
    }

    /// Add environment variable
    pub fn with_env_var<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.general.env_vars.insert(key.into(), value.into());
        self
    }

    /// Build the local transport configuration
    pub fn build(self) -> Result<TransportConfig> {
        let config = TransportConfig {
            transport_type: TransportType::Local,
            ssh: None,
            local: Some(self.config),
            tcp: None,
            wsl: None,
            general: self.general,
        };
        config.validate()?;
        Ok(config)
    }
}

/// TCP transport builder
pub struct TcpTransportBuilder {
    config: TcpConfig,
    general: GeneralConfig,
}

impl TcpTransportBuilder {
    fn new() -> Self {
        Self {
            config: TcpConfig {
                host: String::new(),
                port: 0,
                timeout: 30,
                tls: None,
            },
            general: GeneralConfig::default(),
        }
    }

    /// Set the target host
    pub fn host<S: Into<String>>(mut self, host: S) -> Self {
        self.config.host = host.into();
        self
    }

    /// Set the target port
    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    /// Set connection timeout
    pub fn timeout(mut self, seconds: u64) -> Self {
        self.config.timeout = seconds;
        self
    }

    /// Enable TLS
    pub fn with_tls(self) -> TlsBuilder {
        TlsBuilder::new(self)
    }

    /// Add environment variable
    pub fn with_env_var<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.general.env_vars.insert(key.into(), value.into());
        self
    }

    /// Build the TCP transport configuration
    pub fn build(self) -> Result<TransportConfig> {
        let config = TransportConfig {
            transport_type: TransportType::Tcp,
            ssh: None,
            local: None,
            tcp: Some(self.config),
            wsl: None,
            general: self.general,
        };
        config.validate()?;
        Ok(config)
    }
}

/// TLS configuration builder
pub struct TlsBuilder {
    tcp_builder: TcpTransportBuilder,
    tls_config: TlsConfig,
}

impl TlsBuilder {
    fn new(tcp_builder: TcpTransportBuilder) -> Self {
        Self {
            tcp_builder,
            tls_config: TlsConfig {
                enabled: true,
                verify_cert: true,
                client_cert: None,
                client_key: None,
                ca_cert: None,
            },
        }
    }

    /// Disable certificate verification (insecure)
    pub fn insecure(mut self) -> Self {
        self.tls_config.verify_cert = false;
        self
    }

    /// Set client certificate
    pub fn client_cert<P: Into<PathBuf>>(mut self, cert_path: P) -> Self {
        self.tls_config.client_cert = Some(cert_path.into());
        self
    }

    /// Set client private key
    pub fn client_key<P: Into<PathBuf>>(mut self, key_path: P) -> Self {
        self.tls_config.client_key = Some(key_path.into());
        self
    }

    /// Set CA certificate
    pub fn ca_cert<P: Into<PathBuf>>(mut self, ca_path: P) -> Self {
        self.tls_config.ca_cert = Some(ca_path.into());
        self
    }

    /// Return to TCP builder
    pub fn done(mut self) -> TcpTransportBuilder {
        self.tcp_builder.config.tls = Some(self.tls_config);
        self.tcp_builder
    }
}

/// WSL transport builder
pub struct WslTransportBuilder {
    config: WslConfig,
    general: GeneralConfig,
}

impl WslTransportBuilder {
    fn new() -> Self {
        Self {
            config: WslConfig {
                distribution: None,
                user: None,
                binary_path: None,
                working_dir: None,
            },
            general: GeneralConfig::default(),
        }
    }

    /// Set WSL distribution
    pub fn distribution<S: Into<String>>(mut self, distribution: S) -> Self {
        self.config.distribution = Some(distribution.into());
        self
    }

    /// Set user
    pub fn user<S: Into<String>>(mut self, user: S) -> Self {
        self.config.user = Some(user.into());
        self
    }

    /// Set binary path
    pub fn binary_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.config.binary_path = Some(path.into());
        self
    }

    /// Set working directory
    pub fn working_dir<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.config.working_dir = Some(path.into());
        self
    }

    /// Add environment variable
    pub fn with_env_var<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.general.env_vars.insert(key.into(), value.into());
        self
    }

    /// Build the WSL transport configuration
    pub fn build(self) -> Result<TransportConfig> {
        let config = TransportConfig {
            transport_type: TransportType::Wsl,
            ssh: None,
            local: None,
            tcp: None,
            wsl: Some(self.config),
            general: self.general,
        };
        config.validate()?;
        Ok(config)
    }
}
