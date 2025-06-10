//! Simple client implementation with transport abstraction

use anyhow::Result;
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use yuha_core::message_channel::MessageChannel;
use yuha_core::protocol::{ResponseItem, YuhaRequest, YuhaResponse};

use crate::ClientError;
use crate::transport::{Transport, TransportConfig};

/// Simplified client using request-response protocol with transport abstraction
pub struct SimpleYuhaClientTransport<T: Transport> {
    transport: T,
    message_channel: Option<Arc<Mutex<MessageChannel<T::Stream>>>>,
}

impl<T: Transport> SimpleYuhaClientTransport<T> {
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
    async fn send_request(&self, request: YuhaRequest) -> Result<YuhaResponse, ClientError> {
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
        let request = YuhaRequest::StartPortForward {
            local_port,
            remote_host: remote_host.clone(),
            remote_port,
        };

        match self.send_request(request).await? {
            YuhaResponse::Success => {
                info!(
                    "Port forwarding started: {} -> {}:{}",
                    local_port, remote_host, remote_port
                );
                Ok(())
            }
            YuhaResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Stop port forwarding
    pub async fn stop_port_forward(&self, local_port: u16) -> Result<(), ClientError> {
        let request = YuhaRequest::StopPortForward { local_port };

        match self.send_request(request).await? {
            YuhaResponse::Success => {
                info!("Port forwarding stopped for port {}", local_port);
                Ok(())
            }
            YuhaResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Send port forward data
    pub async fn send_port_forward_data(
        &self,
        connection_id: u32,
        data: Bytes,
    ) -> Result<(), ClientError> {
        let request = YuhaRequest::PortForwardData {
            connection_id,
            data,
        };

        match self.send_request(request).await? {
            YuhaResponse::Success => Ok(()),
            YuhaResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Get clipboard content
    pub async fn get_clipboard(&self) -> Result<String, ClientError> {
        let request = YuhaRequest::GetClipboard;

        match self.send_request(request).await? {
            YuhaResponse::Data { items } => {
                for item in items {
                    if let ResponseItem::ClipboardContent { content } = item {
                        return Ok(content);
                    }
                }
                Err(ClientError::Channel(
                    "No clipboard content in response".to_string(),
                ))
            }
            YuhaResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Set clipboard content
    pub async fn set_clipboard(&self, content: String) -> Result<(), ClientError> {
        let request = YuhaRequest::SetClipboard { content };

        match self.send_request(request).await? {
            YuhaResponse::Success => Ok(()),
            YuhaResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Open browser with URL
    pub async fn open_browser(&self, url: String) -> Result<(), ClientError> {
        let request = YuhaRequest::OpenBrowser { url };

        match self.send_request(request).await? {
            YuhaResponse::Success => Ok(()),
            YuhaResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Poll for data (used for simulating bidirectional communication)
    pub async fn poll_data(&self) -> Result<Vec<ResponseItem>, ClientError> {
        let request = YuhaRequest::PollData;

        match self.send_request(request).await? {
            YuhaResponse::Data { items } => Ok(items),
            YuhaResponse::Error { message } => Err(ClientError::RemoteExecution(message)),
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
) -> Result<SimpleYuhaClientTransport<crate::transport::LocalTransport>, ClientError> {
    use crate::transport::{LocalTransport, LocalTransportConfig};

    let local_config = LocalTransportConfig {
        binary_path,
        args: vec!["--stdio".to_string()],
    };

    let transport = LocalTransport::new(local_config, transport_config);
    let mut client = SimpleYuhaClientTransport::new(transport);
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
) -> Result<SimpleYuhaClientTransport<crate::transport::SshTransport>, ClientError> {
    use crate::transport::{SshTransport, SshTransportConfig};

    let ssh_config = SshTransportConfig {
        host: host.to_string(),
        port,
        username: username.to_string(),
        password: password.map(|s| s.to_string()),
        key_path: key_path.map(|p| p.to_path_buf()),
    };

    let transport = SshTransport::new(ssh_config, transport_config);
    let mut client = SimpleYuhaClientTransport::new(transport);
    client.connect().await?;

    Ok(client)
}
