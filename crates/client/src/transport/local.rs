//! Local transport implementation
//!
//! This module provides a transport that runs the yuha-remote process locally
//! and communicates via stdin/stdout.

use super::{LocalTransportConfig, Transport, TransportConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context as TaskContext, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::{debug, info};

/// Local transport that runs yuha-remote as a subprocess
pub struct LocalTransport {
    config: LocalTransportConfig,
    transport_config: TransportConfig,
}

impl LocalTransport {
    /// Create a new local transport
    pub fn new(config: LocalTransportConfig, transport_config: TransportConfig) -> Self {
        Self {
            config,
            transport_config,
        }
    }
}

#[async_trait]
impl Transport for LocalTransport {
    type Stream = LocalProcessStream;

    async fn connect(&self) -> Result<Self::Stream> {
        info!(
            "Starting local yuha-remote process: {:?}",
            self.config.binary_path
        );

        let mut cmd = Command::new(&self.config.binary_path);

        // Add command line arguments
        for arg in &self.config.args {
            cmd.arg(arg);
        }

        // Set environment variables
        for (key, value) in &self.transport_config.env_vars {
            cmd.env(key, value);
        }

        // Set working directory if specified
        if let Some(ref working_dir) = self.transport_config.working_dir {
            cmd.current_dir(working_dir);
        }

        // Configure stdio
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Spawn the process
        let mut child = cmd.spawn().with_context(|| {
            format!(
                "Failed to spawn yuha-remote at {:?}",
                self.config.binary_path
            )
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin from child process"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout from child process"))?;

        let stderr = child.stderr.take();

        // Spawn a task to log stderr output
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    debug!("yuha-remote stderr: {}", line);
                }
            });
        }

        info!("Local yuha-remote process started successfully");

        Ok(LocalProcessStream {
            child: Some(child),
            stdin,
            stdout,
        })
    }

    fn name(&self) -> &'static str {
        "local"
    }
}

/// Stream adapter for local process communication
pub struct LocalProcessStream {
    child: Option<Child>,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl Drop for LocalProcessStream {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Try to kill the child process
            let _ = child.start_kill();
        }
    }
}

impl AsyncRead for LocalProcessStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdout).poll_read(cx, buf)
    }
}

impl AsyncWrite for LocalProcessStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_local_transport_creation() {
        let config = LocalTransportConfig {
            binary_path: PathBuf::from("yuha-remote"),
            args: vec!["--stdio".to_string()],
        };
        let transport_config = TransportConfig::default();
        let transport = LocalTransport::new(config, transport_config);
        assert_eq!(transport.name(), "local");
    }
}
