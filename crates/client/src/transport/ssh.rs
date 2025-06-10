//! SSH transport implementation
//!
//! This module provides a transport that connects to a remote server via SSH
//! and runs the yuha-remote process.

use super::{SshTransportConfig, Transport, TransportConfig};
use crate::{ClientError, MyHandler, REMOTE_BINARY_PATH, SshChannelAdapter};
use anyhow::{Context, Result};
use async_trait::async_trait;
use russh::client::{AuthResult, Config, Handle, connect};
use std::sync::Arc;
use tracing::info;

/// SSH transport implementation
pub struct SshTransport {
    config: SshTransportConfig,
    transport_config: TransportConfig,
}

impl SshTransport {
    /// Create a new SSH transport
    pub fn new(config: SshTransportConfig, transport_config: TransportConfig) -> Self {
        Self {
            config,
            transport_config,
        }
    }

    /// Transfer binary to remote host and return the path
    async fn transfer_binary_to_remote(
        handle: &Handle<MyHandler>,
        binary_path: &str,
    ) -> Result<String, ClientError> {
        info!("Starting binary transfer to remote host");

        // Read the local binary
        let binary_data = tokio::fs::read(binary_path).await.map_err(|e| {
            ClientError::BinaryTransfer(format!(
                "Failed to read local binary at {}: {}",
                binary_path, e
            ))
        })?;

        info!(
            "Read {} bytes from local binary: {}",
            binary_data.len(),
            binary_path
        );

        // Generate a unique temporary path on the remote
        let remote_temp_path = format!("/tmp/yuha-remote-{}", std::process::id());

        // Use a simpler approach: write the binary directly using cat
        info!(
            "Transferring {} bytes to {}",
            binary_data.len(),
            remote_temp_path
        );

        // Create a channel for the transfer
        let channel = handle.channel_open_session().await.map_err(|e| {
            ClientError::BinaryTransfer(format!("Failed to open transfer channel: {}", e))
        })?;

        // Encode binary as base64
        use base64::Engine;
        let encoded_data = base64::engine::general_purpose::STANDARD.encode(&binary_data);

        // Check if we need to split the transfer
        const MAX_COMMAND_SIZE: usize = 128 * 1024; // 128KB command limit for safety

        if encoded_data.len() <= MAX_COMMAND_SIZE {
            let transfer_command = format!(
                "echo '{}' | base64 -d > {} && chmod +x {} && echo 'Transfer completed'",
                encoded_data, remote_temp_path, remote_temp_path
            );

            // Execute the transfer command
            channel
                .exec(false, transfer_command.as_bytes())
                .await
                .map_err(|e| {
                    ClientError::BinaryTransfer(format!(
                        "Failed to execute transfer command: {}",
                        e
                    ))
                })?;
        } else {
            // For larger files, use chunked transfer
            info!(
                "Binary is large ({} bytes, {} encoded), using chunked transfer",
                binary_data.len(),
                encoded_data.len()
            );

            const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks
            let chunks: Vec<&str> = encoded_data
                .as_bytes()
                .chunks(CHUNK_SIZE)
                .map(|chunk| std::str::from_utf8(chunk).unwrap())
                .collect();

            info!("Splitting into {} chunks", chunks.len());

            // First chunk - create the file
            let first_command =
                format!("echo -n '{}' | base64 -d > {}", chunks[0], remote_temp_path);
            channel
                .exec(false, first_command.as_bytes())
                .await
                .map_err(|e| {
                    ClientError::BinaryTransfer(format!("Failed to start chunked transfer: {}", e))
                })?;

            // Remaining chunks - append to the file
            for (i, chunk) in chunks[1..].iter().enumerate() {
                let append_command =
                    format!("echo -n '{}' | base64 -d >> {}", chunk, remote_temp_path);

                let append_channel = handle.channel_open_session().await.map_err(|e| {
                    ClientError::BinaryTransfer(format!(
                        "Failed to open channel for chunk {}: {}",
                        i + 2,
                        e
                    ))
                })?;

                append_channel
                    .exec(false, append_command.as_bytes())
                    .await
                    .map_err(|e| {
                        ClientError::BinaryTransfer(format!(
                            "Failed to transfer chunk {}: {}",
                            i + 2,
                            e
                        ))
                    })?;
            }

            // Set executable permissions
            let chmod_channel = handle.channel_open_session().await.map_err(|e| {
                ClientError::BinaryTransfer(format!("Failed to open channel for chmod: {}", e))
            })?;

            let chmod_command = format!("chmod +x {}", remote_temp_path);
            chmod_channel
                .exec(false, chmod_command.as_bytes())
                .await
                .map_err(|e| {
                    ClientError::BinaryTransfer(format!(
                        "Failed to set executable permissions: {}",
                        e
                    ))
                })?;
        }

        info!("Binary transferred successfully to {}", remote_temp_path);

        Ok(remote_temp_path)
    }
}

#[async_trait]
impl Transport for SshTransport {
    type Stream = SshChannelAdapter;

    async fn connect(&self) -> Result<Self::Stream> {
        info!(
            "Connecting to {}:{} as {}",
            self.config.host, self.config.port, self.config.username
        );

        let config = Arc::new(Config::default());
        let (handler, data_rx) = MyHandler::new();

        // Connect to SSH server
        let mut handle = connect(config, (&self.config.host[..], self.config.port), handler)
            .await
            .context("Failed to connect to SSH server")?;

        // Authenticate
        if let Some(ref password) = self.config.password {
            let auth_result = handle
                .authenticate_password(&self.config.username, password)
                .await
                .context("Failed to authenticate with password")?;
            if !matches!(auth_result, AuthResult::Success) {
                anyhow::bail!("Password authentication failed");
            }
        } else if let Some(ref key_path) = self.config.key_path {
            let key_str = std::fs::read_to_string(key_path)
                .with_context(|| format!("Failed to read key file: {:?}", key_path))?;
            let russh_key = russh::keys::PrivateKey::from_openssh(&key_str)
                .context("Failed to parse SSH key")?;
            let key_with_hash = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(russh_key), None);
            let auth_result = handle
                .authenticate_publickey(&self.config.username, key_with_hash)
                .await
                .context("Failed to authenticate with key")?;
            if !matches!(auth_result, AuthResult::Success) {
                anyhow::bail!("Key authentication failed");
            }
        } else {
            anyhow::bail!("No authentication method provided");
        }

        info!("Authentication successful");

        // Determine the remote binary path
        let remote_path = if self.transport_config.auto_upload_binary {
            info!("Auto-uploading binary enabled, transferring binary to remote");
            let binary_path = if let Some(ref path) = self.transport_config.remote_binary_path {
                path.to_string_lossy().to_string()
            } else {
                REMOTE_BINARY_PATH.to_string()
            };
            Self::transfer_binary_to_remote(&handle, &binary_path).await?
        } else {
            info!("Using pre-installed binary at /usr/local/bin/yuha-remote");
            "/usr/local/bin/yuha-remote".to_string()
        };

        // Create a channel
        let channel = handle
            .channel_open_session()
            .await
            .context("Failed to open SSH channel")?;
        let channel_id = channel.id();

        // Build the command with environment variables
        let mut env_prefix = String::new();
        for (key, value) in &self.transport_config.env_vars {
            env_prefix.push_str(&format!("{}={} ", key, value));
        }

        // Execute the remote command
        let stderr_log = if self.transport_config.auto_upload_binary {
            format!("/tmp/remote_stderr_{}.log", std::process::id())
        } else {
            "/tmp/remote_stderr.log".to_string()
        };

        let command = format!("{}{} --stdio 2>{}", env_prefix, remote_path, stderr_log);
        info!("Executing remote command: {}", command);

        // Execute the command using channel exec
        channel
            .exec(false, command.as_bytes())
            .await
            .with_context(|| format!("Failed to execute remote command: {}", command))?;

        info!("Remote command executed successfully");

        // Create the adapter
        let ssh_adapter = SshChannelAdapter::new(handle, channel_id, data_rx);

        Ok(ssh_adapter)
    }

    fn name(&self) -> &'static str {
        "ssh"
    }
}
