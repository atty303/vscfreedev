//! Daemon client for communicating with yuha-daemon
//!
//! This module provides a client interface for sending requests to
//! the yuha daemon and receiving responses.

use crate::daemon_protocol::{
    DaemonCommand, DaemonRequest, DaemonResponse, SessionDetails, SessionSummary,
};
use anyhow::Result;
use bytes::Bytes;
use std::path::PathBuf;
use tracing::{debug, info};
use yuha_core::{message_channel::MessageChannel, session::SessionId};

use crate::ClientError;

#[cfg(unix)]
use crate::transport::Transport;
#[cfg(unix)]
use crate::transport::unix::{UnixTransport, UnixTransportConfig};

/// Client for communicating with the yuha daemon
pub struct DaemonClient {
    #[cfg(unix)]
    channel: MessageChannel<tokio::net::UnixStream>,
    #[cfg(windows)]
    channel: MessageChannel<crate::transport::windows::NamedPipeStream>,
}

impl DaemonClient {
    /// Connect to the daemon
    pub async fn connect(socket_path: Option<PathBuf>) -> Result<Self, ClientError> {
        let socket_path = socket_path.unwrap_or_else(|| {
            if cfg!(unix) {
                PathBuf::from("/tmp/yuha-daemon.sock")
            } else {
                PathBuf::from(r"\\.\pipe\yuha-daemon")
            }
        });

        info!("Connecting to daemon at {:?}", socket_path);

        #[cfg(unix)]
        {
            let config = UnixTransportConfig {
                socket_path: socket_path.clone(),
            };
            let transport =
                UnixTransport::new(config, crate::transport::TransportConfig::default());
            let stream = transport.connect().await.map_err(|e| {
                ClientError::Connection(format!("Failed to connect to daemon: {}", e))
            })?;
            let channel = MessageChannel::new_with_stream(stream);
            Ok(Self { channel })
        }

        #[cfg(windows)]
        {
            use crate::transport::windows::{WindowsTransport, WindowsTransportConfig};

            let config = WindowsTransportConfig {
                pipe_name: socket_path.to_string_lossy().to_string(),
            };
            let transport =
                WindowsTransport::new(config, crate::transport::TransportConfig::default());
            let stream = transport.connect().await.map_err(|e| {
                ClientError::Connection(format!("Failed to connect to daemon: {}", e))
            })?;
            let channel = MessageChannel::new_with_stream(stream);
            Ok(Self { channel })
        }
    }

    /// Send a request to the daemon and wait for response
    async fn send_request(
        &mut self,
        request: DaemonRequest,
    ) -> Result<DaemonResponse, ClientError> {
        debug!("Sending request to daemon: {:?}", request);

        // Serialize request
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| ClientError::Channel(format!("Failed to serialize request: {}", e)))?;

        // Send request
        self.channel
            .send(Bytes::from(request_bytes))
            .await
            .map_err(|e| ClientError::Channel(format!("Failed to send request: {}", e)))?;

        // Receive response
        let response_bytes = self
            .channel
            .receive()
            .await
            .map_err(|e| ClientError::Channel(format!("Failed to receive response: {}", e)))?;

        // Deserialize response
        let response: DaemonResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| ClientError::Channel(format!("Failed to deserialize response: {}", e)))?;

        debug!("Received response from daemon: {:?}", response);
        Ok(response)
    }

    /// Ping the daemon to check if it's alive
    pub async fn ping(&mut self) -> Result<bool, ClientError> {
        match self.send_request(DaemonRequest::Ping).await? {
            DaemonResponse::Pong => Ok(true),
            _ => Ok(false),
        }
    }

    /// Create a new session
    pub async fn create_session(
        &mut self,
        name: String,
        transport_config: yuha_core::transport::TransportConfig,
        tags: Vec<String>,
        description: Option<String>,
    ) -> Result<(SessionId, bool), ClientError> {
        let request = DaemonRequest::CreateSession {
            name,
            transport_config,
            tags,
            description,
        };

        match self.send_request(request).await? {
            DaemonResponse::SessionCreated { session_id, reused } => Ok((session_id, reused)),
            DaemonResponse::Error { code, message } => {
                Err(ClientError::DaemonError { code, message })
            }
            _ => Err(ClientError::Channel(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    /// Connect to an existing session or create a new one
    pub async fn connect_session(
        &mut self,
        name: String,
        transport_config: yuha_core::transport::TransportConfig,
    ) -> Result<(SessionId, bool), ClientError> {
        let request = DaemonRequest::ConnectSession {
            name,
            transport_config,
        };

        match self.send_request(request).await? {
            DaemonResponse::SessionCreated { session_id, reused } => Ok((session_id, reused)),
            DaemonResponse::Error { code, message } => {
                Err(ClientError::DaemonError { code, message })
            }
            _ => Err(ClientError::Channel(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    /// Disconnect from a session
    pub async fn disconnect_session(&mut self, session_id: SessionId) -> Result<(), ClientError> {
        let request = DaemonRequest::DisconnectSession { session_id };

        match self.send_request(request).await? {
            DaemonResponse::SessionDisconnected => Ok(()),
            DaemonResponse::Error { code, message } => {
                Err(ClientError::DaemonError { code, message })
            }
            _ => Err(ClientError::Channel(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    /// List all sessions
    pub async fn list_sessions(&mut self) -> Result<Vec<SessionSummary>, ClientError> {
        match self.send_request(DaemonRequest::ListSessions).await? {
            DaemonResponse::SessionList { sessions } => Ok(sessions),
            DaemonResponse::Error { code, message } => {
                Err(ClientError::DaemonError { code, message })
            }
            _ => Err(ClientError::Channel(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    /// Get session information
    pub async fn get_session_info(
        &mut self,
        session_id: SessionId,
    ) -> Result<SessionDetails, ClientError> {
        let request = DaemonRequest::GetSessionInfo { session_id };

        match self.send_request(request).await? {
            DaemonResponse::SessionInfo { session } => Ok(*session),
            DaemonResponse::Error { code, message } => {
                Err(ClientError::DaemonError { code, message })
            }
            _ => Err(ClientError::Channel(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    /// Execute a command on a session
    pub async fn execute_command(
        &mut self,
        session_id: SessionId,
        command: DaemonCommand,
    ) -> Result<crate::daemon_protocol::CommandResult, ClientError> {
        let request = DaemonRequest::ExecuteCommand {
            session_id,
            command,
        };

        match self.send_request(request).await? {
            DaemonResponse::CommandSuccess { result } => Ok(result),
            DaemonResponse::Error { code, message } => {
                Err(ClientError::DaemonError { code, message })
            }
            _ => Err(ClientError::Channel(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    /// Shutdown the daemon
    pub async fn shutdown(&mut self) -> Result<(), ClientError> {
        match self.send_request(DaemonRequest::Shutdown).await? {
            DaemonResponse::ShuttingDown => Ok(()),
            DaemonResponse::Error { code, message } => {
                Err(ClientError::DaemonError { code, message })
            }
            _ => Err(ClientError::Channel(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }
}

/// Wrapper client for session-based operations
pub struct DaemonSessionClient {
    daemon_client: DaemonClient,
    session_id: SessionId,
}

impl DaemonSessionClient {
    /// Create a new session client
    pub fn new(daemon_client: DaemonClient, session_id: SessionId) -> Self {
        Self {
            daemon_client,
            session_id,
        }
    }

    /// Get clipboard content
    pub async fn get_clipboard(&mut self) -> Result<String, ClientError> {
        match self
            .daemon_client
            .execute_command(self.session_id, DaemonCommand::GetClipboard)
            .await?
        {
            crate::daemon_protocol::CommandResult::ClipboardContent { content } => Ok(content),
            _ => Err(ClientError::Channel(
                "Unexpected command result".to_string(),
            )),
        }
    }

    /// Set clipboard content
    pub async fn set_clipboard(&mut self, content: String) -> Result<(), ClientError> {
        match self
            .daemon_client
            .execute_command(self.session_id, DaemonCommand::SetClipboard { content })
            .await?
        {
            crate::daemon_protocol::CommandResult::ClipboardSet => Ok(()),
            _ => Err(ClientError::Channel(
                "Unexpected command result".to_string(),
            )),
        }
    }

    /// Open browser with URL
    pub async fn open_browser(&mut self, url: String) -> Result<(), ClientError> {
        match self
            .daemon_client
            .execute_command(self.session_id, DaemonCommand::OpenBrowser { url })
            .await?
        {
            crate::daemon_protocol::CommandResult::BrowserOpened => Ok(()),
            _ => Err(ClientError::Channel(
                "Unexpected command result".to_string(),
            )),
        }
    }

    /// Start port forwarding
    pub async fn start_port_forward(
        &mut self,
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    ) -> Result<(), ClientError> {
        match self
            .daemon_client
            .execute_command(
                self.session_id,
                DaemonCommand::StartPortForward {
                    local_port,
                    remote_host,
                    remote_port,
                },
            )
            .await?
        {
            crate::daemon_protocol::CommandResult::PortForwardStarted => Ok(()),
            _ => Err(ClientError::Channel(
                "Unexpected command result".to_string(),
            )),
        }
    }

    /// Stop port forwarding
    pub async fn stop_port_forward(&mut self, local_port: u16) -> Result<(), ClientError> {
        match self
            .daemon_client
            .execute_command(
                self.session_id,
                DaemonCommand::StopPortForward { local_port },
            )
            .await?
        {
            crate::daemon_protocol::CommandResult::PortForwardStopped => Ok(()),
            _ => Err(ClientError::Channel(
                "Unexpected command result".to_string(),
            )),
        }
    }
}
