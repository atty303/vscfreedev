//! TCP transport implementation
//!
//! This module provides a transport that connects directly to a yuha-remote
//! process via TCP socket connection.

use super::{Transport, TransportConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::{TcpStream, lookup_host};
use tokio::time::timeout;
use tracing::{debug, info, warn};

/// TCP transport configuration
#[derive(Debug, Clone)]
pub struct TcpTransportConfig {
    pub host: String,
    pub port: u16,
    pub connection_timeout: Duration,
    pub keepalive: bool,
}

impl Default for TcpTransportConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 9999,
            connection_timeout: Duration::from_secs(30),
            keepalive: true,
        }
    }
}

/// TCP transport implementation
#[derive(Debug)]
pub struct TcpTransport {
    config: TcpTransportConfig,
    #[allow(dead_code)]
    transport_config: TransportConfig,
}

impl TcpTransport {
    /// Create a new TCP transport
    pub fn new(config: TcpTransportConfig, transport_config: TransportConfig) -> Self {
        Self {
            config,
            transport_config,
        }
    }

    /// Connect to the remote TCP server
    async fn connect_tcp(&self) -> Result<TcpStream> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        info!("Connecting to TCP server at {}", addr);

        // Resolve the address
        let socket_addrs: Vec<SocketAddr> = lookup_host(&addr)
            .await
            .with_context(|| format!("Failed to resolve address: {}", addr))?
            .collect();

        if socket_addrs.is_empty() {
            anyhow::bail!("No addresses resolved for: {}", addr);
        }

        debug!("Resolved addresses: {:?}", socket_addrs);

        // Try connecting to each resolved address
        let mut last_error = None;
        for socket_addr in socket_addrs {
            debug!("Attempting to connect to {}", socket_addr);

            match timeout(
                self.config.connection_timeout,
                TcpStream::connect(socket_addr),
            )
            .await
            {
                Ok(Ok(stream)) => {
                    info!("Successfully connected to {}", socket_addr);

                    // Configure keep-alive if enabled
                    if self.config.keepalive {
                        let socket = socket2::Socket::from(stream.into_std()?);
                        socket.set_keepalive(true)?;
                        let stream = TcpStream::from_std(socket.into())?;
                        return Ok(stream);
                    }

                    return Ok(stream);
                }
                Ok(Err(e)) => {
                    warn!("Failed to connect to {}: {}", socket_addr, e);
                    last_error = Some(e.into());
                }
                Err(_) => {
                    let timeout_error = anyhow::anyhow!(
                        "Connection timeout after {:?} to {}",
                        self.config.connection_timeout,
                        socket_addr
                    );
                    warn!("{}", timeout_error);
                    last_error = Some(timeout_error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!("Failed to connect to any resolved address for {}", addr)
        }))
    }
}

#[async_trait]
impl Transport for TcpTransport {
    type Stream = TcpStream;

    async fn connect(&self) -> Result<Self::Stream> {
        info!(
            "Establishing TCP connection to {}:{}",
            self.config.host, self.config.port
        );

        let stream = self.connect_tcp().await?;

        // Set TCP no-delay for better latency
        stream
            .set_nodelay(true)
            .with_context(|| "Failed to set TCP_NODELAY on connection")?;

        info!(
            "TCP connection established successfully to {}:{}",
            self.config.host, self.config.port
        );

        Ok(stream)
    }

    fn name(&self) -> &'static str {
        "tcp"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_tcp_transport_creation() {
        let config = TcpTransportConfig {
            host: "localhost".to_string(),
            port: 8080,
            connection_timeout: Duration::from_secs(10),
            keepalive: true,
        };
        let transport_config = TransportConfig::default();
        let transport = TcpTransport::new(config, transport_config);
        assert_eq!(transport.name(), "tcp");
    }

    #[test]
    fn test_default_tcp_config() {
        let config = TcpTransportConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 9999);
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
        assert!(config.keepalive);
    }
}
