//! IPC functionality for remote process communication
//!
//! This module provides IPC server and client capabilities allowing
//! shell commands to communicate with the running remote process.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{RwLock, mpsc};
use tracing::{error, info};
use yuha_core::protocol::ResponseBuffer;

/// IPC command that can be sent from shell to remote process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcCommand {
    /// Get current clipboard content
    GetClipboard,
    /// Set clipboard content
    SetClipboard { content: String },
    /// Open URL in browser
    OpenBrowser { url: String },
    /// Send message to client
    SendToClient { message: String },
    /// Get status of remote process
    Status,
    /// Ping the remote process
    Ping,
}

/// IPC response from remote process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    /// Success with optional data
    Success { data: Option<String> },
    /// Error with message
    Error { message: String },
    /// Status information
    Status {
        uptime: u64,
        connected_clients: u32,
        active_port_forwards: u32,
    },
    /// Pong response to ping
    Pong,
}

/// IPC server for handling shell commands
pub struct IpcServer {
    socket_path: PathBuf,
    response_buffer: Arc<RwLock<ResponseBuffer>>,
    client_sender: Option<mpsc::UnboundedSender<String>>,
    uptime_start: std::time::Instant,
}

impl IpcServer {
    /// Create a new IPC server
    pub fn new(socket_path: PathBuf, response_buffer: Arc<RwLock<ResponseBuffer>>) -> Self {
        Self {
            socket_path,
            response_buffer,
            client_sender: None,
            uptime_start: std::time::Instant::now(),
        }
    }

    /// Set client sender for forwarding messages
    pub fn set_client_sender(&mut self, sender: mpsc::UnboundedSender<String>) {
        self.client_sender = Some(sender);
    }

    /// Start the IPC server
    pub async fn start(&self) -> Result<()> {
        // Remove existing socket if it exists
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        // Create parent directory if it doesn't exist
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        info!("IPC server listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let response_buffer = self.response_buffer.clone();
                    let client_sender = self.client_sender.clone();
                    let uptime_start = self.uptime_start;

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(
                            stream,
                            response_buffer,
                            client_sender,
                            uptime_start,
                        )
                        .await
                        {
                            error!("IPC connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("IPC accept error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a single IPC connection
    async fn handle_connection(
        stream: UnixStream,
        response_buffer: Arc<RwLock<ResponseBuffer>>,
        client_sender: Option<mpsc::UnboundedSender<String>>,
        uptime_start: std::time::Instant,
    ) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let command: IpcCommand = match serde_json::from_str(line.trim()) {
                        Ok(cmd) => cmd,
                        Err(e) => {
                            let response = IpcResponse::Error {
                                message: format!("Invalid command format: {}", e),
                            };
                            Self::send_response(&mut writer, &response).await?;
                            continue;
                        }
                    };

                    let response = Self::handle_command(
                        command,
                        &response_buffer,
                        &client_sender,
                        uptime_start,
                    )
                    .await;

                    Self::send_response(&mut writer, &response).await?;
                }
                Err(e) => {
                    error!("IPC read error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle an IPC command
    async fn handle_command(
        command: IpcCommand,
        _response_buffer: &Arc<RwLock<ResponseBuffer>>,
        client_sender: &Option<mpsc::UnboundedSender<String>>,
        uptime_start: std::time::Instant,
    ) -> IpcResponse {
        match command {
            IpcCommand::GetClipboard => match yuha_core::clipboard::get_clipboard() {
                Ok(content) => IpcResponse::Success {
                    data: Some(content),
                },
                Err(e) => IpcResponse::Error {
                    message: format!("Failed to get clipboard: {}", e),
                },
            },
            IpcCommand::SetClipboard { content } => {
                match yuha_core::clipboard::set_clipboard(&content) {
                    Ok(()) => IpcResponse::Success { data: None },
                    Err(e) => IpcResponse::Error {
                        message: format!("Failed to set clipboard: {}", e),
                    },
                }
            }
            IpcCommand::OpenBrowser { url } => match yuha_core::browser::open_url(&url).await {
                Ok(()) => IpcResponse::Success { data: None },
                Err(e) => IpcResponse::Error {
                    message: format!("Failed to open browser: {}", e),
                },
            },
            IpcCommand::SendToClient { message } => {
                if let Some(sender) = client_sender {
                    match sender.send(message) {
                        Ok(()) => IpcResponse::Success { data: None },
                        Err(e) => IpcResponse::Error {
                            message: format!("Failed to send to client: {}", e),
                        },
                    }
                } else {
                    IpcResponse::Error {
                        message: "No client connected".to_string(),
                    }
                }
            }
            IpcCommand::Status => {
                IpcResponse::Status {
                    uptime: uptime_start.elapsed().as_secs(),
                    connected_clients: if client_sender.is_some() { 1 } else { 0 },
                    active_port_forwards: 0, // TODO: Track active port forwards
                }
            }
            IpcCommand::Ping => IpcResponse::Pong,
        }
    }

    /// Send an IPC response
    async fn send_response(
        writer: &mut tokio::net::unix::OwnedWriteHalf,
        response: &IpcResponse,
    ) -> Result<()> {
        let json = serde_json::to_string(response)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        Ok(())
    }
}

/// IPC client for sending commands to remote process
pub struct IpcClient {
    socket_path: PathBuf,
}

impl IpcClient {
    /// Create a new IPC client
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Send a command and get response
    pub async fn send_command(&self, command: IpcCommand) -> Result<IpcResponse> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;

        // Send command
        let json = serde_json::to_string(&command)?;
        stream.write_all(json.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;

        // Read response
        let mut reader = BufReader::new(&mut stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let response: IpcResponse = serde_json::from_str(line.trim())?;
        Ok(response)
    }

    /// Check if remote process is running
    pub async fn ping(&self) -> bool {
        matches!(
            self.send_command(IpcCommand::Ping).await,
            Ok(IpcResponse::Pong)
        )
    }

    /// Get clipboard content
    pub async fn get_clipboard(&self) -> Result<String> {
        match self.send_command(IpcCommand::GetClipboard).await? {
            IpcResponse::Success {
                data: Some(content),
            } => Ok(content),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    /// Set clipboard content
    pub async fn set_clipboard(&self, content: &str) -> Result<()> {
        match self
            .send_command(IpcCommand::SetClipboard {
                content: content.to_string(),
            })
            .await?
        {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    /// Open URL in browser
    pub async fn open_browser(&self, url: &str) -> Result<()> {
        match self
            .send_command(IpcCommand::OpenBrowser {
                url: url.to_string(),
            })
            .await?
        {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    /// Send message to client
    pub async fn send_to_client(&self, message: &str) -> Result<()> {
        match self
            .send_command(IpcCommand::SendToClient {
                message: message.to_string(),
            })
            .await?
        {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }

    /// Get status
    pub async fn status(&self) -> Result<(u64, u32, u32)> {
        match self.send_command(IpcCommand::Status).await? {
            IpcResponse::Status {
                uptime,
                connected_clients,
                active_port_forwards,
            } => Ok((uptime, connected_clients, active_port_forwards)),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => Err(anyhow::anyhow!("Unexpected response")),
        }
    }
}

/// Get default IPC socket path
pub fn get_default_ipc_socket_path() -> PathBuf {
    if cfg!(unix) {
        PathBuf::from("/tmp/yuha-remote.sock")
    } else {
        // For Windows, use a named pipe
        PathBuf::from(r"\\.\pipe\yuha-remote")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_command_serialization() {
        let cmd = IpcCommand::SetClipboard {
            content: "test".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: IpcCommand = serde_json::from_str(&json).unwrap();
        matches!(parsed, IpcCommand::SetClipboard { .. });
    }

    #[test]
    fn test_ipc_response_serialization() {
        let resp = IpcResponse::Success {
            data: Some("test".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();
        matches!(parsed, IpcResponse::Success { .. });
    }

    #[test]
    fn test_default_socket_path() {
        let path = get_default_ipc_socket_path();
        assert!(!path.as_os_str().is_empty());
    }
}
