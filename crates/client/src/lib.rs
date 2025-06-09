//! Client-side library for yuha

use anyhow::Result;
use russh::ChannelId;
use russh::client::{AuthResult, Config, Handle, Handler, Session, connect};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, error, info};
use yuha_core::message_channel::MessageChannel;

pub mod simple_client;

/// Path to the remote binary built by build.rs
pub const REMOTE_BINARY_PATH: &str = env!("YUHA_REMOTE_BINARY_PATH");

/// Error types for the client
#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("SSH error: {0}")]
    Ssh(#[from] russh::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Remote execution error: {0}")]
    RemoteExecution(String),

    #[error("Channel error: {0}")]
    Channel(String),

    #[error("Key error: {0}")]
    Key(#[from] russh_keys::Error),

    #[error("Binary transfer error: {0}")]
    BinaryTransfer(String),
}

/// Handler for SSH client events
pub(crate) struct MyHandler {
    data_tx: Arc<Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>>,
}

impl MyHandler {
    fn new() -> (Self, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                data_tx: Arc::new(Mutex::new(Some(tx))),
            },
            rx,
        )
    }
}

#[async_trait::async_trait]
impl Handler for MyHandler {
    type Error = russh::Error;

    #[allow(clippy::manual_async_fn)]
    fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send {
        async { Ok(true) }
    }

    #[allow(clippy::manual_async_fn)]
    fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        async move {
            debug!("RusshHandler: Received {} bytes", data.len());
            let data_tx_guard = self.data_tx.lock().await;
            if let Some(ref tx) = *data_tx_guard {
                if let Err(e) = tx.send(data.to_vec()) {
                    error!("RusshHandler: Failed to send data: {}", e);
                }
            }
            Ok(())
        }
    }
}

/// An adapter that implements AsyncRead and AsyncWrite for a russh channel
pub struct SshChannelAdapter {
    handle: Handle<MyHandler>,
    channel_id: ChannelId,
    read_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    read_buf: Vec<u8>,
}

impl SshChannelAdapter {
    /// Create a new SSH channel adapter
    pub(crate) fn new(
        handle: Handle<MyHandler>,
        channel_id: ChannelId,
        read_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    ) -> Self {
        Self {
            handle,
            channel_id,
            read_rx,
            read_buf: Vec::new(),
        }
    }
}

impl AsyncRead for SshChannelAdapter {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // Try to read from our internal buffer first
        if !self.read_buf.is_empty() {
            let to_copy = std::cmp::min(buf.remaining(), self.read_buf.len());
            let data = self.read_buf.drain(..to_copy).collect::<Vec<u8>>();
            buf.put_slice(&data);
            debug!(
                "SshChannelAdapter::poll_read returned {} bytes from buffer",
                to_copy
            );
            return Poll::Ready(Ok(()));
        }

        // Try to receive new data from channel
        match self.read_rx.try_recv() {
            Ok(data) => {
                debug!("SshChannelAdapter::poll_read got {} bytes", data.len());
                let to_copy = std::cmp::min(buf.remaining(), data.len());
                buf.put_slice(&data[..to_copy]);

                // Store remainder in buffer if any
                if to_copy < data.len() {
                    self.read_buf.extend_from_slice(&data[to_copy..]);
                }

                debug!("SshChannelAdapter::poll_read returned {} bytes", to_copy);
                Poll::Ready(Ok(()))
            }
            Err(mpsc::error::TryRecvError::Empty) => Poll::Pending,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                debug!("SshChannelAdapter::poll_read channel closed");
                Poll::Ready(Ok(()))
            }
        }
    }
}

impl AsyncWrite for SshChannelAdapter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        debug!(
            "SshChannelAdapter::poll_write called with {} bytes",
            buf.len()
        );

        // Create future to send data
        let send_fut = self.handle.data(self.channel_id, buf.to_vec().into());
        tokio::pin!(send_fut);

        match send_fut.poll(cx) {
            Poll::Ready(Ok(_)) => {
                debug!(
                    "SshChannelAdapter::poll_write sent {} bytes successfully",
                    buf.len()
                );
                Poll::Ready(Ok(buf.len()))
            }
            Poll::Ready(Err(e)) => {
                error!("SshChannelAdapter::poll_write error: {:?}", e);
                Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    format!("SSH data send failed: {:?}", e),
                )))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
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

    // Create a channel for verification first
    let verify_channel = handle.channel_open_session().await.map_err(|e| {
        ClientError::BinaryTransfer(format!("Failed to open verification channel: {}", e))
    })?;

    // Check if the target directory exists
    verify_channel
        .exec(false, b"ls -la /tmp/")
        .await
        .map_err(|e| {
            ClientError::BinaryTransfer(format!("Failed to verify tmp directory: {}", e))
        })?;

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

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
        debug!(
            "Transferring binary ({} bytes, {} encoded) in one command",
            binary_data.len(),
            encoded_data.len()
        );

        let transfer_command = format!(
            "echo '{}' | base64 -d > {} && chmod +x {} && echo 'Transfer completed'",
            encoded_data, remote_temp_path, remote_temp_path
        );

        debug!(
            "Executing transfer command (length: {})",
            transfer_command.len()
        );

        // Execute the transfer command
        channel
            .exec(false, transfer_command.as_bytes())
            .await
            .map_err(|e| {
                ClientError::BinaryTransfer(format!("Failed to execute transfer command: {}", e))
            })?;

        // Give the command time to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
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
        let first_command = format!("echo -n '{}' | base64 -d > {}", chunks[0], remote_temp_path);
        channel
            .exec(false, first_command.as_bytes())
            .await
            .map_err(|e| {
                ClientError::BinaryTransfer(format!("Failed to start chunked transfer: {}", e))
            })?;

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Remaining chunks - append to the file
        for (i, chunk) in chunks[1..].iter().enumerate() {
            debug!("Transferring chunk {} of {}", i + 2, chunks.len());

            let append_command = format!("echo -n '{}' | base64 -d >> {}", chunk, remote_temp_path);

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

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
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
                ClientError::BinaryTransfer(format!("Failed to set executable permissions: {}", e))
            })?;

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    // Verify the transfer by checking if the file exists and is executable
    let verify_channel2 = handle.channel_open_session().await.map_err(|e| {
        ClientError::BinaryTransfer(format!("Failed to open second verification channel: {}", e))
    })?;

    let verify_command = format!(
        "ls -la {} && echo 'File exists' && {} --help && echo 'Binary is executable'",
        remote_temp_path, remote_temp_path
    );
    verify_channel2
        .exec(false, verify_command.as_bytes())
        .await
        .map_err(|e| {
            ClientError::BinaryTransfer(format!("Failed to verify transferred binary: {}", e))
        })?;

    info!("Binary transferred successfully to {}", remote_temp_path);

    Ok(remote_temp_path)
}

/// Public client interface
pub mod client {
    use super::*;

    /// Connect to a remote host via SSH and return a message channel
    pub async fn connect_ssh(
        host: &str,
        port: u16,
        username: &str,
        password: Option<&str>,
        key_path: Option<&Path>,
    ) -> Result<MessageChannel<SshChannelAdapter>, ClientError> {
        connect_ssh_with_options(host, port, username, password, key_path, false).await
    }

    /// Connect to a remote host via SSH with options and return a message channel
    pub async fn connect_ssh_with_options(
        host: &str,
        port: u16,
        username: &str,
        password: Option<&str>,
        key_path: Option<&Path>,
        auto_upload_binary: bool,
    ) -> Result<MessageChannel<SshChannelAdapter>, ClientError> {
        info!("Connecting to {}:{} as {}", host, port, username);

        let config = Arc::new(Config::default());
        let (handler, data_rx) = MyHandler::new();

        // Connect to SSH server
        let mut handle = connect(config, (host, port), handler).await?;

        // Authenticate
        if let Some(password) = password {
            debug!("Authenticating with password");
            let auth_result = handle.authenticate_password(username, password).await?;
            if !matches!(auth_result, AuthResult::Success) {
                return Err(ClientError::Connection(
                    "Password authentication failed".to_string(),
                ));
            }
        } else if let Some(key_path) = key_path {
            debug!("Authenticating with key: {:?}", key_path);
            let key_str = std::fs::read_to_string(key_path)?;
            let _key = russh_keys::decode_secret_key(&key_str, None)?;
            // Convert from russh_keys::PrivateKey to russh::keys::PrivateKey
            let russh_key = russh::keys::PrivateKey::from_openssh(&key_str)
                .map_err(|e| ClientError::Connection(format!("Failed to parse key: {}", e)))?;
            let key_with_hash = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(russh_key), None);
            let auth_result = handle
                .authenticate_publickey(username, key_with_hash)
                .await?;
            if !matches!(auth_result, AuthResult::Success) {
                return Err(ClientError::Connection(
                    "Key authentication failed".to_string(),
                ));
            }
        } else {
            return Err(ClientError::Connection(
                "No authentication method provided".to_string(),
            ));
        }

        info!("Authentication successful");

        // Determine the remote binary path
        let remote_path = if auto_upload_binary {
            info!("Auto-uploading binary enabled, transferring binary to remote");
            transfer_binary_to_remote(&handle, REMOTE_BINARY_PATH).await?
        } else {
            info!("Using pre-installed binary at /usr/local/bin/yuha-remote");
            "/usr/local/bin/yuha-remote".to_string()
        };

        // Create a channel
        let channel = handle.channel_open_session().await?;
        let channel_id = channel.id();
        debug!("Created SSH channel: {:?}", channel_id);

        // Execute the remote command (now always uses simple protocol)
        let command = format!("{} --stdio 2>/tmp/remote_stderr.log", remote_path);
        debug!("Executing remote command: {}", command);

        // Execute the command using channel exec
        match channel.exec(false, command.as_bytes()).await {
            Ok(_) => {
                debug!("Remote command executed successfully");

                // Give the remote server a moment to start up
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
            Err(e) => {
                error!("Failed to execute remote command '{}': {}", command, e);
                return Err(ClientError::RemoteExecution(format!(
                    "Failed to execute remote command '{}': {}",
                    command, e
                )));
            }
        }

        // Create the adapter
        let ssh_adapter = SshChannelAdapter::new(handle, channel_id, data_rx);

        // Create a message channel using binary mode
        let message_channel = MessageChannel::new_with_stream(ssh_adapter);

        Ok(message_channel)
    }

    /// Get the path to the built remote binary
    pub fn get_remote_binary_path() -> &'static str {
        REMOTE_BINARY_PATH
    }
}
