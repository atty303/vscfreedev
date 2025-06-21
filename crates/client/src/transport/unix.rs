//! Unix domain socket transport implementation
//!
//! This module provides a transport that connects to a yuha daemon
//! via Unix domain socket for IPC communication.

use super::{Transport, TransportConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::net::UnixStream;
use tracing::{debug, info};

/// Unix socket transport configuration
#[derive(Debug, Clone)]
pub struct UnixTransportConfig {
    pub socket_path: PathBuf,
}

impl Default for UnixTransportConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/tmp/yuha-daemon.sock"),
        }
    }
}

/// Unix socket transport implementation
#[derive(Debug)]
pub struct UnixTransport {
    config: UnixTransportConfig,
    #[allow(dead_code)]
    transport_config: TransportConfig,
}

impl UnixTransport {
    /// Create a new Unix socket transport
    pub fn new(config: UnixTransportConfig, transport_config: TransportConfig) -> Self {
        Self {
            config,
            transport_config,
        }
    }
}

#[async_trait]
impl Transport for UnixTransport {
    type Stream = UnixStream;

    async fn connect(&self) -> Result<Self::Stream> {
        info!("Connecting to Unix socket at {:?}", self.config.socket_path);

        let stream = UnixStream::connect(&self.config.socket_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to connect to Unix socket at {:?}",
                    self.config.socket_path
                )
            })?;

        debug!("Unix socket connection established");
        Ok(stream)
    }

    fn name(&self) -> &'static str {
        "unix"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unix_transport_creation() {
        let config = UnixTransportConfig {
            socket_path: PathBuf::from("/tmp/test.sock"),
        };
        let transport_config = TransportConfig::default();
        let transport = UnixTransport::new(config, transport_config);
        assert_eq!(transport.name(), "unix");
    }

    #[test]
    fn test_default_unix_config() {
        let config = UnixTransportConfig::default();
        assert_eq!(config.socket_path, PathBuf::from("/tmp/yuha-daemon.sock"));
    }
}
