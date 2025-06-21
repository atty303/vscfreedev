//! WSL transport implementation
//!
//! This module provides a transport that runs yuha-remote in Windows Subsystem for Linux (WSL).

use super::shared::{ProcessStream, configure_command, spawn_stderr_logger};
use super::{Transport, TransportConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

/// WSL transport configuration
#[derive(Debug, Clone)]
pub struct WslTransportConfig {
    /// WSL distribution name (e.g., "Ubuntu", "Ubuntu-20.04")
    pub distribution: Option<String>,
    /// User to execute as in WSL
    pub user: Option<String>,
    /// Path to yuha-remote binary in WSL filesystem
    pub binary_path: PathBuf,
    /// Working directory in WSL
    pub working_dir: Option<PathBuf>,
}

impl Default for WslTransportConfig {
    fn default() -> Self {
        Self {
            distribution: None,
            user: None,
            binary_path: PathBuf::from("yuha-remote"),
            working_dir: None,
        }
    }
}

/// WSL transport implementation
#[derive(Debug)]
pub struct WslTransport {
    config: WslTransportConfig,
    transport_config: TransportConfig,
}

impl WslTransport {
    /// Create a new WSL transport
    pub fn new(config: WslTransportConfig, transport_config: TransportConfig) -> Self {
        Self {
            config,
            transport_config,
        }
    }

    /// Check if WSL is available on the system
    pub async fn is_wsl_available() -> bool {
        if !cfg!(windows) {
            debug!("WSL is only available on Windows");
            return false;
        }

        // Try to run `wsl --version` to check if WSL is installed
        match Command::new("wsl").args(["--version"]).output().await {
            Ok(output) => {
                if output.status.success() {
                    debug!("WSL is available");
                    true
                } else {
                    debug!("WSL command failed: {:?}", output);
                    false
                }
            }
            Err(e) => {
                debug!("WSL not found: {}", e);
                false
            }
        }
    }

    /// List available WSL distributions
    pub async fn list_distributions() -> Result<Vec<String>> {
        if !cfg!(windows) {
            return Err(anyhow::anyhow!("WSL is only available on Windows"));
        }

        let output = Command::new("wsl")
            .args(["--list", "--quiet"])
            .output()
            .await
            .context("Failed to execute wsl --list command")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "WSL list command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let distributions = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();

        debug!("Available WSL distributions: {:?}", distributions);
        Ok(distributions)
    }

    /// Build the WSL command
    fn build_command(&self) -> Command {
        let mut cmd = Command::new("wsl");

        // Add distribution flag if specified
        if let Some(ref distribution) = self.config.distribution {
            cmd.args(["--distribution", distribution]);
        }

        // Add user flag if specified
        if let Some(ref user) = self.config.user {
            cmd.args(["--user", user]);
        }

        // Add working directory if specified
        if let Some(ref working_dir) = self.config.working_dir {
            cmd.args(["--cd", &working_dir.to_string_lossy()]);
        }

        // Add the command to execute
        cmd.arg(self.config.binary_path.to_string_lossy().to_string());
        cmd.arg("--stdio");

        cmd
    }
}

#[async_trait]
impl Transport for WslTransport {
    type Stream = ProcessStream;

    async fn connect(&self) -> Result<Self::Stream> {
        if !cfg!(windows) {
            return Err(anyhow::anyhow!(
                "WSL transport is only available on Windows"
            ));
        }

        // Check if WSL is available
        if !Self::is_wsl_available().await {
            return Err(anyhow::anyhow!("WSL is not available on this system"));
        }

        info!(
            "Starting yuha-remote in WSL distribution: {:?}",
            self.config.distribution.as_deref().unwrap_or("default")
        );

        let mut cmd = self.build_command();

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

        debug!("WSL command: {:?}", cmd);

        // Spawn the process
        let mut child = cmd.spawn().with_context(|| {
            format!(
                "Failed to spawn yuha-remote in WSL: {:?}",
                self.config.binary_path
            )
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin from WSL child process"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout from WSL child process"))?;

        let stderr = child.stderr.take();

        // Spawn a task to log stderr output
        if let Some(stderr) = stderr {
            let prefix = format!(
                "yuha-remote WSL({})",
                self.config.distribution.as_deref().unwrap_or("default")
            );
            spawn_stderr_logger(stderr, &prefix);
        }

        info!("WSL yuha-remote process started successfully");

        Ok(ProcessStream::new(child, stdin, stdout))
    }

    fn name(&self) -> &'static str {
        "wsl"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wsl_transport_creation() {
        let config = WslTransportConfig {
            distribution: Some("Ubuntu".to_string()),
            user: Some("user".to_string()),
            binary_path: PathBuf::from("yuha-remote"),
            working_dir: None,
        };
        let transport_config = TransportConfig::default();
        let transport = WslTransport::new(config, transport_config);
        assert_eq!(transport.name(), "wsl");
    }

    #[test]
    fn test_default_wsl_config() {
        let config = WslTransportConfig::default();
        assert!(config.distribution.is_none());
        assert!(config.user.is_none());
        assert_eq!(config.binary_path, PathBuf::from("yuha-remote"));
        assert!(config.working_dir.is_none());
    }

    #[tokio::test]
    async fn test_wsl_availability() {
        // This test will only pass on Windows with WSL installed
        let available = WslTransport::is_wsl_available().await;

        if cfg!(windows) {
            // On Windows, this might be true or false depending on WSL installation
            println!("WSL available: {}", available);
        } else {
            // On non-Windows systems, should always be false
            assert!(!available);
        }
    }
}
