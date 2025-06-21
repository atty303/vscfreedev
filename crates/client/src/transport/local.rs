//! Local transport implementation
//!
//! This module provides a transport that runs the yuha-remote process locally
//! and communicates via stdin/stdout.

use super::shared::{ProcessStream, configure_command, spawn_stderr_logger};
use super::{LocalTransportConfig, Transport, TransportConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::process::Command;
use tracing::info;

/// Local transport that runs yuha-remote as a subprocess
#[derive(Debug)]
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
    type Stream = ProcessStream;

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

        // Configure common command options
        configure_command(
            &mut cmd,
            &self.transport_config.env_vars,
            &self.transport_config.working_dir,
        );

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
            spawn_stderr_logger(stderr, "yuha-remote");
        }

        info!("Local yuha-remote process started successfully");

        Ok(ProcessStream::new(child, stdin, stdout))
    }

    fn name(&self) -> &'static str {
        "local"
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
