use anyhow::Result;
use bytes::Bytes;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use yuha_core::message_channel::MessageChannel;
use yuha_core::protocol::{ResponseItem, YuhaRequest, YuhaResponse};

use crate::{ClientError, SshChannelAdapter};

/// Simplified client using request-response protocol
pub struct SimpleYuhaClient<T> {
    message_channel: Arc<Mutex<MessageChannel<T>>>,
}

impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static> SimpleYuhaClient<T> {
    /// Create a new simple client
    pub fn new(message_channel: MessageChannel<T>) -> Self {
        Self {
            message_channel: Arc::new(Mutex::new(message_channel)),
        }
    }

    /// Send a request and wait for response
    async fn send_request(&self, request: YuhaRequest) -> Result<YuhaResponse, ClientError> {
        let mut channel = self.message_channel.lock().await;

        debug!("Sending request: {:?}", request);
        channel
            .send_request(&request)
            .await
            .map_err(|e| ClientError::Channel(format!("Failed to send request: {}", e)))?;

        // Wait for response with timeout - give more time for server processing
        let response = tokio::time::timeout(Duration::from_secs(60), channel.receive_response())
            .await
            .map_err(|_| ClientError::Channel("Request timeout after 60 seconds".to_string()))?
            .map_err(|e| ClientError::Channel(format!("Failed to receive response: {}", e)))?;

        debug!("Received response: {:?}", response);
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
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }

            // Small delay to prevent busy polling
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

/// Connect to a remote host via SSH and return a SimpleYuhaClient
pub async fn connect_ssh(
    host: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
    key_path: Option<&Path>,
) -> Result<SimpleYuhaClient<SshChannelAdapter>, ClientError> {
    let message_channel =
        crate::client::connect_ssh(host, port, username, password, key_path).await?;
    Ok(SimpleYuhaClient::new(message_channel))
}

/// Connect to a remote host via SSH with auto-upload option and return a SimpleYuhaClient
pub async fn connect_ssh_with_auto_upload(
    host: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
    key_path: Option<&Path>,
    auto_upload_binary: bool,
) -> Result<SimpleYuhaClient<SshChannelAdapter>, ClientError> {
    let message_channel = crate::client::connect_ssh_with_options(
        host,
        port,
        username,
        password,
        key_path,
        auto_upload_binary,
    )
    .await?;
    Ok(SimpleYuhaClient::new(message_channel))
}
