//! Windows named pipe transport implementation
//!
//! This module provides a transport that connects to a yuha daemon
//! via Windows named pipes for IPC communication.

use super::{Transport, TransportConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::windows::named_pipe::ClientOptions;
use tracing::{debug, info};

/// Windows named pipe transport configuration
#[derive(Debug, Clone)]
pub struct WindowsTransportConfig {
    pub pipe_name: String,
}

impl Default for WindowsTransportConfig {
    fn default() -> Self {
        Self {
            pipe_name: r"\\.\pipe\yuha-daemon".to_string(),
        }
    }
}

/// Windows named pipe transport implementation
#[derive(Debug)]
pub struct WindowsTransport {
    config: WindowsTransportConfig,
    #[allow(dead_code)]
    transport_config: TransportConfig,
}

impl WindowsTransport {
    /// Create a new Windows named pipe transport
    pub fn new(config: WindowsTransportConfig, transport_config: TransportConfig) -> Self {
        Self {
            config,
            transport_config,
        }
    }
}

/// Wrapper for Windows named pipe client to implement AsyncRead + AsyncWrite
pub struct NamedPipeStream {
    inner: tokio::net::windows::named_pipe::NamedPipeClient,
}

impl AsyncRead for NamedPipeStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for NamedPipeStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[async_trait]
impl Transport for WindowsTransport {
    type Stream = NamedPipeStream;

    async fn connect(&self) -> Result<Self::Stream> {
        info!(
            "Connecting to Windows named pipe: {}",
            self.config.pipe_name
        );

        let client = ClientOptions::new()
            .open(&self.config.pipe_name)
            .await
            .with_context(|| {
                format!(
                    "Failed to connect to Windows named pipe: {}",
                    self.config.pipe_name
                )
            })?;

        debug!("Windows named pipe connection established");
        Ok(NamedPipeStream { inner: client })
    }

    fn name(&self) -> &'static str {
        "windows"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_transport_creation() {
        let config = WindowsTransportConfig {
            pipe_name: r"\\.\pipe\test".to_string(),
        };
        let transport_config = TransportConfig::default();
        let transport = WindowsTransport::new(config, transport_config);
        assert_eq!(transport.name(), "windows");
    }

    #[test]
    fn test_default_windows_config() {
        let config = WindowsTransportConfig::default();
        assert_eq!(config.pipe_name, r"\\.\pipe\yuha-daemon");
    }
}
