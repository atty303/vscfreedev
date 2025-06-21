//! Transport factory for creating transport instances
//!
//! This module provides a unified factory for creating transport instances
//! from transport configurations.

use crate::transport::tcp::TcpTransportConfig;
use crate::transport::wsl::WslTransportConfig;
use crate::transport::{
    LocalTransport, LocalTransportConfig, SshTransport, SshTransportConfig, TcpTransport,
    Transport, TransportConfig, WslTransport,
};
use anyhow::Result;
use std::time::Duration;
use tracing::{debug, info};
use yuha_core::transport::{TransportConfig as CoreTransportConfig, TransportType};

/// Enum that can hold any transport type
#[derive(Debug)]
pub enum AnyTransport {
    Local(LocalTransport),
    Ssh(SshTransport),
    Tcp(TcpTransport),
    Wsl(WslTransport),
}

impl AnyTransport {
    /// Get the name of the transport
    pub fn name(&self) -> &'static str {
        match self {
            AnyTransport::Local(t) => t.name(),
            AnyTransport::Ssh(t) => t.name(),
            AnyTransport::Tcp(t) => t.name(),
            AnyTransport::Wsl(t) => t.name(),
        }
    }
}

/// Factory for creating transport instances from configurations
pub struct ClientTransportFactory;

impl ClientTransportFactory {
    /// Create a transport instance from a core transport configuration
    pub fn create_transport(config: &CoreTransportConfig) -> Result<AnyTransport> {
        match config.transport_type {
            TransportType::Local => Ok(AnyTransport::Local(Self::create_local_transport(config)?)),
            TransportType::Ssh => Ok(AnyTransport::Ssh(Self::create_ssh_transport(config)?)),
            TransportType::Tcp => Ok(AnyTransport::Tcp(Self::create_tcp_transport(config)?)),
            TransportType::Wsl => Ok(AnyTransport::Wsl(Self::create_wsl_transport(config)?)),
        }
    }

    /// Create a local transport
    fn create_local_transport(config: &CoreTransportConfig) -> Result<LocalTransport> {
        let local_config = config
            .local
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Local transport configuration is required"))?;

        let transport_config = TransportConfig {
            remote_binary_path: config.general.remote_binary_path.clone(),
            auto_upload_binary: false, // Local transport doesn't need to upload
            env_vars: config
                .general
                .env_vars
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            working_dir: local_config.working_dir.clone(),
        };

        let local_transport_config = LocalTransportConfig {
            binary_path: local_config.binary_path.clone(),
            args: local_config.args.clone(),
        };

        info!(
            "Creating local transport: {:?}",
            local_transport_config.binary_path
        );
        Ok(LocalTransport::new(
            local_transport_config,
            transport_config,
        ))
    }

    /// Create an SSH transport
    fn create_ssh_transport(config: &CoreTransportConfig) -> Result<SshTransport> {
        let ssh_config = config
            .ssh
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("SSH transport configuration is required"))?;

        let transport_config = TransportConfig {
            remote_binary_path: config.general.remote_binary_path.clone(),
            auto_upload_binary: ssh_config.auto_upload_binary,
            env_vars: config
                .general
                .env_vars
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            working_dir: None, // SSH transport doesn't use working_dir in the same way
        };

        let ssh_transport_config = SshTransportConfig {
            host: ssh_config.host.clone(),
            port: ssh_config.port,
            username: ssh_config.username.clone(),
            password: ssh_config.password.clone(),
            key_path: ssh_config.key_path.clone(),
        };

        info!(
            "Creating SSH transport: {}@{}:{}",
            ssh_transport_config.username, ssh_transport_config.host, ssh_transport_config.port
        );
        Ok(SshTransport::new(ssh_transport_config, transport_config))
    }

    /// Create a TCP transport
    fn create_tcp_transport(config: &CoreTransportConfig) -> Result<TcpTransport> {
        let tcp_config = config
            .tcp
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("TCP transport configuration is required"))?;

        let transport_config = TransportConfig {
            remote_binary_path: config.general.remote_binary_path.clone(),
            auto_upload_binary: false, // TCP transport connects to existing server
            env_vars: config
                .general
                .env_vars
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            working_dir: None,
        };

        let tcp_transport_config = TcpTransportConfig {
            host: tcp_config.host.clone(),
            port: tcp_config.port,
            connection_timeout: Duration::from_secs(tcp_config.timeout),
            keepalive: true, // Enable keepalive
        };

        info!(
            "Creating TCP transport: {}:{}",
            tcp_transport_config.host, tcp_transport_config.port
        );
        Ok(TcpTransport::new(tcp_transport_config, transport_config))
    }

    /// Create a WSL transport
    fn create_wsl_transport(config: &CoreTransportConfig) -> Result<WslTransport> {
        let wsl_config = config
            .wsl
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("WSL transport configuration is required"))?;

        let transport_config = TransportConfig {
            remote_binary_path: config.general.remote_binary_path.clone(),
            auto_upload_binary: false, // WSL uses local filesystem
            env_vars: config
                .general
                .env_vars
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            working_dir: wsl_config.working_dir.clone(),
        };

        let wsl_transport_config = WslTransportConfig {
            distribution: wsl_config.distribution.clone(),
            user: wsl_config.user.clone(),
            binary_path: wsl_config
                .binary_path
                .clone()
                .unwrap_or_else(|| std::path::PathBuf::from("yuha-remote")),
            working_dir: wsl_config.working_dir.clone(),
        };

        info!(
            "Creating WSL transport: distribution={:?}, user={:?}",
            wsl_transport_config.distribution, wsl_transport_config.user
        );
        Ok(WslTransport::new(wsl_transport_config, transport_config))
    }

    /// Auto-detect best available transport
    pub async fn auto_detect_transport() -> Result<TransportType> {
        debug!("Auto-detecting best available transport");

        // Check WSL availability on Windows
        if cfg!(windows) && WslTransport::is_wsl_available().await {
            info!("Auto-detected: WSL transport is available");
            return Ok(TransportType::Wsl);
        }

        // Check if local yuha-remote binary is available
        if Self::is_local_binary_available().await {
            info!("Auto-detected: Local transport is available");
            return Ok(TransportType::Local);
        }

        // Default to local transport even if binary is not found
        // (user might specify the path)
        info!("Auto-detected: Defaulting to local transport");
        Ok(TransportType::Local)
    }

    /// Check if local yuha-remote binary is available
    async fn is_local_binary_available() -> bool {
        // Check common locations for yuha-remote
        let possible_paths = vec![
            "yuha-remote",
            "./yuha-remote",
            "/usr/local/bin/yuha-remote",
            "/usr/bin/yuha-remote",
        ];

        for path in possible_paths {
            if tokio::fs::metadata(path).await.is_ok() {
                debug!("Found yuha-remote binary at: {}", path);
                return true;
            }
        }

        debug!("No yuha-remote binary found in common locations");
        false
    }

    /// Get supported transport types for the current platform
    pub fn supported_transports() -> Vec<TransportType> {
        let mut transports = vec![TransportType::Local, TransportType::Ssh, TransportType::Tcp];

        if cfg!(windows) {
            transports.push(TransportType::Wsl);
        }

        transports
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yuha_core::transport::{GeneralConfig, LocalConfig};

    #[test]
    fn test_supported_transports() {
        let transports = ClientTransportFactory::supported_transports();
        assert!(transports.contains(&TransportType::Local));
        assert!(transports.contains(&TransportType::Ssh));
        assert!(transports.contains(&TransportType::Tcp));

        if cfg!(windows) {
            assert!(transports.contains(&TransportType::Wsl));
        }
    }

    #[tokio::test]
    async fn test_auto_detect_transport() {
        let transport_type = ClientTransportFactory::auto_detect_transport()
            .await
            .unwrap();

        // Should always detect at least local transport
        assert!(matches!(
            transport_type,
            TransportType::Local | TransportType::Wsl
        ));
    }

    #[test]
    fn test_create_local_transport() {
        let config = CoreTransportConfig {
            transport_type: TransportType::Local,
            ssh: None,
            local: Some(yuha_core::transport::LocalConfig {
                binary_path: std::path::PathBuf::from("test-binary"),
                args: vec!["--test".to_string()],
                working_dir: None,
            }),
            tcp: None,
            wsl: None,
            general: GeneralConfig::default(),
        };

        let result = ClientTransportFactory::create_local_transport(&config);
        assert!(result.is_ok());
    }
}
