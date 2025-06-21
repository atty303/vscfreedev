//! # Client with Transport Abstraction
//!
//! This module provides a client implementation that works with any
//! transport type. It abstracts away the complexity of different connection methods
//! while providing a clean, consistent API for client applications.
//!
//! ## Key Features
//!
//! - **Transport Agnostic**: Works with SSH, Local, TCP, and other transports
//! - **Clean API**: Request-response pattern with async/await support
//! - **Error Handling**: Comprehensive error reporting and recovery
//! - **Connection Management**: Automatic connection handling and lifecycle
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use yuha_client::Client;
//! use yuha_client::transport::local::LocalTransport;
//! use yuha_client::transport::{LocalTransportConfig, TransportConfig};
//!
//! // Create transport and client
//! let local_config = LocalTransportConfig::default();
//! let transport_config = TransportConfig::default();
//! let transport = LocalTransport::new(local_config, transport_config);
//! let mut client = Client::new(transport);
//!
//! // Connect and use
//! client.connect().await?;
//! let clipboard = client.get_clipboard().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Type Alias
//!
//! The type is also available as `Client<T>` for direct usage.

use anyhow::Result;
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use yuha_core::message_channel::MessageChannel;
use yuha_core::protocol::{ProtocolRequest, ProtocolResponse, ResponseItem};

use crate::ClientError;
use crate::transport::{Transport, TransportConfig};

/// Client using request-response protocol with transport abstraction.
///
/// This client provides a high-level interface for communicating with remote Yuha servers
/// through any transport implementation. It handles connection management, request serialization,
/// and response deserialization automatically.
///
/// # Type Parameters
///
/// - `T`: Transport implementation that handles the underlying connection
///
/// # Example
///
/// ```rust,no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use yuha_client::Client;
/// use yuha_client::transport::local::LocalTransport;
/// use yuha_client::transport::{LocalTransportConfig, TransportConfig};
///
/// let local_config = LocalTransportConfig::default();
/// let transport_config = TransportConfig::default();
/// let transport = LocalTransport::new(local_config, transport_config);
/// let mut client = Client::new(transport);
/// client.connect().await?;
/// # Ok(())
/// # }
/// ```
pub struct Client<T: Transport> {
    /// The underlying transport for communication
    transport: T,
    /// Message channel for sending/receiving protocol messages
    message_channel: Option<Arc<Mutex<MessageChannel<T::Stream>>>>,
}

impl<T: Transport> Client<T> {
    /// Create a new client with the given transport
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            message_channel: None,
        }
    }

    /// Connect to the remote server
    pub async fn connect(&mut self) -> Result<(), ClientError> {
        info!("Connecting using {} transport", self.transport.name());

        let stream =
            self.transport.connect().await.map_err(|e| {
                ClientError::Connection(format!("Transport connection failed: {}", e))
            })?;

        let message_channel = MessageChannel::new_with_stream(stream);
        self.message_channel = Some(Arc::new(Mutex::new(message_channel)));

        info!(
            "Connected successfully via {} transport",
            self.transport.name()
        );
        Ok(())
    }

    /// Send a request and wait for response
    async fn send_request(
        &self,
        request: ProtocolRequest,
    ) -> Result<ProtocolResponse, ClientError> {
        let channel = self
            .message_channel
            .as_ref()
            .ok_or_else(|| ClientError::Connection("Not connected".to_string()))?;

        let mut channel = channel.lock().await;

        channel
            .send_request(&request)
            .await
            .map_err(|e| ClientError::Channel(format!("Failed to send request: {}", e)))?;

        let response = channel
            .receive_response()
            .await
            .map_err(|e| ClientError::Channel(format!("Failed to receive response: {}", e)))?;

        Ok(response)
    }

    /// Start port forwarding
    pub async fn start_port_forward(
        &self,
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    ) -> Result<(), ClientError> {
        let request = ProtocolRequest::StartPortForward {
            local_port,
            remote_host: remote_host.clone(),
            remote_port,
        };

        match self.send_request(request).await? {
            ProtocolResponse::Success => {
                info!(
                    "Port forwarding started: {} -> {}:{}",
                    local_port, remote_host, remote_port
                );
                Ok(())
            }
            ProtocolResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Stop port forwarding
    pub async fn stop_port_forward(&self, local_port: u16) -> Result<(), ClientError> {
        let request = ProtocolRequest::StopPortForward { local_port };

        match self.send_request(request).await? {
            ProtocolResponse::Success => {
                info!("Port forwarding stopped for port {}", local_port);
                Ok(())
            }
            ProtocolResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Send port forward data
    pub async fn send_port_forward_data(
        &self,
        connection_id: u32,
        data: Bytes,
    ) -> Result<(), ClientError> {
        let request = ProtocolRequest::PortForwardData {
            connection_id,
            data,
        };

        match self.send_request(request).await? {
            ProtocolResponse::Success => Ok(()),
            ProtocolResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Get clipboard content
    pub async fn get_clipboard(&self) -> Result<String, ClientError> {
        let request = ProtocolRequest::GetClipboard;

        match self.send_request(request).await? {
            ProtocolResponse::Data { items } => {
                for item in items {
                    if let ResponseItem::ClipboardContent { content } = item {
                        return Ok(content);
                    }
                }
                Err(ClientError::Channel(
                    "No clipboard content in response".to_string(),
                ))
            }
            ProtocolResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Set clipboard content
    pub async fn set_clipboard(&self, content: String) -> Result<(), ClientError> {
        let request = ProtocolRequest::SetClipboard { content };

        match self.send_request(request).await? {
            ProtocolResponse::Success => Ok(()),
            ProtocolResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Open browser with URL
    pub async fn open_browser(&self, url: String) -> Result<(), ClientError> {
        let request = ProtocolRequest::OpenBrowser { url };

        match self.send_request(request).await? {
            ProtocolResponse::Success => Ok(()),
            ProtocolResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Poll for data (used for simulating bidirectional communication)
    pub async fn poll_data(&self) -> Result<Vec<ResponseItem>, ClientError> {
        let request = ProtocolRequest::PollData;

        match self.send_request(request).await? {
            ProtocolResponse::Data { items } => Ok(items),
            ProtocolResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Start polling loop for receiving data from remote
    pub async fn start_polling_loop<F>(&self, mut handler: F) -> Result<(), ClientError>
    where
        F: FnMut(ResponseItem) -> bool + Send + 'static,
    {
        loop {
            match self.poll_data().await {
                Ok(items) => {
                    for item in items {
                        let should_continue = handler(item);
                        if !should_continue {
                            return Ok(());
                        }
                    }
                }
                Err(e) => {
                    warn!("Error polling data: {}", e);
                    // Continue immediately on error, server will handle timing
                }
            }
            // No sleep here - long polling is handled by server
        }
    }
}

/// Helper function to create a client with local transport
pub async fn connect_local(
    binary_path: std::path::PathBuf,
    transport_config: TransportConfig,
) -> Result<Client<crate::transport::LocalTransport>, ClientError> {
    use crate::transport::{LocalTransport, LocalTransportConfig};

    let local_config = LocalTransportConfig {
        binary_path,
        args: vec!["--stdio".to_string()],
    };

    let transport = LocalTransport::new(local_config, transport_config);
    let mut client = Client::new(transport);
    client.connect().await?;

    Ok(client)
}

/// Helper function to create a client with SSH transport
pub async fn connect_ssh_transport(
    host: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
    key_path: Option<&std::path::Path>,
    transport_config: TransportConfig,
) -> Result<Client<crate::transport::SshTransport>, ClientError> {
    use crate::transport::{SshTransport, SshTransportConfig};

    let ssh_config = SshTransportConfig {
        host: host.to_string(),
        port,
        username: username.to_string(),
        password: password.map(|s| s.to_string()),
        key_path: key_path.map(|p| p.to_path_buf()),
    };

    let transport = SshTransport::new(ssh_config, transport_config);
    let mut client = Client::new(transport);
    client.connect().await?;

    Ok(client)
}
