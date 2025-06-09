//! Client-side library for yuha

use anyhow::Result;
use bytes::Bytes;
use russh::ChannelId;
use russh::client::{AuthResult, Config, Handle, Handler, Session, connect};
use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::sync::{Mutex, RwLock, mpsc};
use tracing::{debug, error, info, warn};
use yuha_core::message_channel::MessageChannel;
use yuha_core::port_forward::{PortForwardManager, PortForwardMessage};

/// Type alias for active connections map  
type ActiveConnections = Arc<RwLock<HashMap<u16, HashMap<u32, mpsc::UnboundedSender<Bytes>>>>>;

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

/// Client with port forwarding capabilities
pub struct YuhaClient {
    message_channel: Arc<Mutex<MessageChannel<SshChannelAdapter>>>,
    port_forward_manager: PortForwardManager,
    // Track active connections: local_port -> (connection_id -> sender)
    active_connections: ActiveConnections,
    next_connection_id: Arc<RwLock<u32>>,
    // Channel for control message responses
    control_response_rx: Arc<Mutex<mpsc::UnboundedReceiver<PortForwardMessage>>>,
    // Flag to track if message handler has been started
    message_handler_started: Arc<RwLock<bool>>,
}

impl YuhaClient {
    /// Create a new client with an existing message channel
    pub fn new(message_channel: MessageChannel<SshChannelAdapter>) -> Self {
        let (_control_response_tx, control_response_rx) = mpsc::unbounded_channel();

        Self {
            message_channel: Arc::new(Mutex::new(message_channel)),
            port_forward_manager: PortForwardManager::new(),
            active_connections: Arc::new(RwLock::new(HashMap::new())),
            next_connection_id: Arc::new(RwLock::new(1)),
            control_response_rx: Arc::new(Mutex::new(control_response_rx)),
            message_handler_started: Arc::new(RwLock::new(false)),
        }
    }

    /// Start the port forward message handler (only when needed)
    async fn ensure_message_handler_started(&self) {
        let mut started = self.message_handler_started.write().await;
        if *started {
            debug!("Message handler already started, returning");
            return;
        }
        debug!("Starting message handler for the first time");
        *started = true;

        let (control_response_tx, new_control_rx) = mpsc::unbounded_channel();

        // Replace the control response receiver
        {
            let mut control_rx = self.control_response_rx.lock().await;
            *control_rx = new_control_rx;
        }

        let message_channel_clone = self.message_channel.clone();
        let active_connections_clone = self.active_connections.clone();

        // Use a one-shot channel to ensure the background task is ready before proceeding
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

        debug!("Spawning background message handler task");
        tokio::spawn(async move {
            debug!("Background message handler task started, entering receive loop");

            // Signal that the task is ready
            let _ = ready_tx.send(());

            loop {
                debug!("Message handler waiting for next message from remote...");
                let message_bytes = {
                    let mut channel = message_channel_clone.lock().await;
                    match channel.receive().await {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            error!("Failed to receive message from remote: {}", e);
                            break;
                        }
                    }
                };

                let message_str = String::from_utf8_lossy(&message_bytes);
                debug!("Message handler received message: {}", message_str);

                // Try to parse as port forward message
                if let Ok(port_forward_msg) =
                    serde_json::from_str::<PortForwardMessage>(&message_str)
                {
                    debug!(
                        "Successfully parsed as port forward message: {:?}",
                        port_forward_msg
                    );
                    match port_forward_msg {
                        PortForwardMessage::StartResponse { .. }
                        | PortForwardMessage::StopResponse { .. } => {
                            // Route control responses to the control channel
                            debug!(
                                "Routing control response to control channel: {:?}",
                                port_forward_msg
                            );
                            if let Err(e) = control_response_tx.send(port_forward_msg) {
                                error!("Failed to send control response: {}", e);
                            } else {
                                debug!("Control response sent successfully");
                            }
                        }
                        PortForwardMessage::Data {
                            local_port,
                            connection_id,
                            data,
                        } => {
                            debug!(
                                "Received {} bytes for connection {} on port {}",
                                data.len(),
                                connection_id,
                                local_port
                            );

                            let connections = active_connections_clone.read().await;
                            if let Some(port_connections) = connections.get(&local_port) {
                                if let Some(sender) = port_connections.get(&connection_id) {
                                    if let Err(e) = sender.send(Bytes::from(data)) {
                                        warn!(
                                            "Failed to send data to connection {}: {}",
                                            connection_id, e
                                        );
                                    }
                                }
                            }
                        }
                        PortForwardMessage::CloseConnection {
                            local_port,
                            connection_id,
                        } => {
                            debug!(
                                "Remote closed connection {} for port {}",
                                connection_id, local_port
                            );

                            let mut connections = active_connections_clone.write().await;
                            if let Some(port_connections) = connections.get_mut(&local_port) {
                                port_connections.remove(&connection_id);
                            }
                        }
                        _ => {
                            debug!("Received other message type: {:?}", port_forward_msg);
                        }
                    }
                } else {
                    debug!("Received non-port-forward message: {}", message_str);
                }
            }
        });

        // Wait for the background task to be ready before proceeding
        debug!("Waiting for background message handler to be ready...");
        if (ready_rx.await).is_err() {
            error!("Background message handler failed to start");
        } else {
            debug!("Background message handler is ready");
        }
    }

    /// Get next connection ID
    #[allow(dead_code)]
    async fn next_connection_id(&self) -> u32 {
        let mut id = self.next_connection_id.write().await;
        let current = *id;
        *id += 1;
        current
    }

    /// Start port forwarding from local port to remote host:port
    pub async fn start_port_forward(
        &self,
        local_port: u16,
        remote_host: &str,
        remote_port: u16,
    ) -> Result<(), ClientError> {
        // Use direct request-response instead of background handler for simplicity
        debug!("Starting port forward using direct request-response approach");

        // Create the port forward request
        let request = PortForwardMessage::StartRequest {
            local_port,
            remote_host: remote_host.to_string(),
            remote_port,
        };

        // Serialize and send the request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| ClientError::Channel(format!("Failed to serialize request: {}", e)))?;

        // Send the request and wait for response directly
        let response_bytes = {
            let mut channel = self.message_channel.lock().await;
            debug!("Sending port forward request: {}", request_json);
            channel
                .send(Bytes::from(request_json))
                .await
                .map_err(|e| ClientError::Channel(format!("Failed to send request: {}", e)))?;
            debug!("Port forward request sent successfully");

            debug!("Waiting for direct response...");
            tokio::time::timeout(Duration::from_secs(15), channel.receive())
                .await
                .map_err(|_| ClientError::Channel("Response timeout after 15 seconds".to_string()))?
                .map_err(|e| ClientError::Channel(format!("Failed to receive response: {}", e)))?
        };

        // Parse the response
        let response_str = String::from_utf8_lossy(&response_bytes);
        debug!("Received response: {}", response_str);

        let response: PortForwardMessage = serde_json::from_str(&response_str)
            .map_err(|e| ClientError::Channel(format!("Failed to parse response: {}", e)))?;
        debug!("Parsed response: {:?}", response);

        match response {
            PortForwardMessage::StartResponse { success: true, .. } => {
                // Start local port forwarding
                self.port_forward_manager
                    .start_forward(local_port, remote_host.to_string(), remote_port)
                    .await
                    .map_err(|e| {
                        ClientError::Channel(format!("Failed to start local forwarding: {}", e))
                    })?;

                // Initialize connection map for this port
                {
                    let mut connections = self.active_connections.write().await;
                    connections.insert(local_port, HashMap::new());
                }

                // Start accepting connections
                self.start_accepting_connections(local_port).await?;

                info!(
                    "Port forwarding started: {} -> {}:{}",
                    local_port, remote_host, remote_port
                );
                Ok(())
            }
            PortForwardMessage::StartResponse {
                success: false,
                error,
                ..
            } => Err(ClientError::RemoteExecution(error.unwrap_or_else(|| {
                "Unknown error starting port forward".to_string()
            }))),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Start accepting connections for a port forward
    async fn start_accepting_connections(&self, local_port: u16) -> Result<(), ClientError> {
        let port_forward = self
            .port_forward_manager
            .get_forward(local_port)
            .await
            .ok_or_else(|| ClientError::Channel("Port forward not found".to_string()))?;

        let message_channel = self.message_channel.clone();
        let active_connections = self.active_connections.clone();
        let next_connection_id = self.next_connection_id.clone();

        tokio::spawn(async move {
            loop {
                match port_forward.listener.accept().await {
                    Ok((mut stream, addr)) => {
                        let connection_id = {
                            let mut id = next_connection_id.write().await;
                            let current = *id;
                            *id += 1;
                            current
                        };

                        info!(
                            "New connection {} from {} on port {}",
                            connection_id, addr, local_port
                        );

                        // Send new connection message
                        let new_conn_msg = PortForwardMessage::NewConnection {
                            local_port,
                            connection_id,
                        };

                        if let Ok(msg_json) = serde_json::to_string(&new_conn_msg) {
                            let mut channel = message_channel.lock().await;
                            if let Err(e) = channel.send(Bytes::from(msg_json)).await {
                                error!("Failed to send new connection message: {}", e);
                                continue;
                            }
                        }

                        // Create channel for receiving data from remote
                        let (tx, mut rx) = mpsc::unbounded_channel();
                        {
                            let mut connections = active_connections.write().await;
                            if let Some(port_connections) = connections.get_mut(&local_port) {
                                port_connections.insert(connection_id, tx);
                            }
                        }

                        let message_channel_clone = message_channel.clone();
                        let active_connections_clone = active_connections.clone();

                        // Handle this connection
                        tokio::spawn(async move {
                            let mut buf = [0; 4096];
                            loop {
                                tokio::select! {
                                    // Read from TCP stream and send to remote
                                    result = stream.read(&mut buf) => {
                                        match result {
                                            Ok(0) => {
                                                debug!("Connection {} closed by client", connection_id);
                                                break;
                                            }
                                            Ok(n) => {
                                                let data_msg = PortForwardMessage::Data {
                                                    local_port,
                                                    connection_id,
                                                    data: buf[..n].to_vec(),
                                                };

                                                if let Ok(msg_json) = serde_json::to_string(&data_msg) {
                                                    let mut channel = message_channel_clone.lock().await;
                                                    if let Err(e) = channel.send(Bytes::from(msg_json)).await {
                                                        error!("Failed to send data message: {}", e);
                                                        break;
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                error!("Error reading from connection {}: {}", connection_id, e);
                                                break;
                                            }
                                        }
                                    }
                                    // Receive data from remote and write to TCP stream
                                    data = rx.recv() => {
                                        match data {
                                            Some(data) => {
                                                if let Err(e) = stream.write_all(&data).await {
                                                    error!("Error writing to connection {}: {}", connection_id, e);
                                                    break;
                                                }
                                            }
                                            None => {
                                                debug!("Data channel closed for connection {}", connection_id);
                                                break;
                                            }
                                        }
                                    }
                                }
                            }

                            // Clean up connection
                            {
                                let mut connections = active_connections_clone.write().await;
                                if let Some(port_connections) = connections.get_mut(&local_port) {
                                    port_connections.remove(&connection_id);
                                }
                            }

                            // Send close connection message
                            let close_msg = PortForwardMessage::CloseConnection {
                                local_port,
                                connection_id,
                            };

                            if let Ok(msg_json) = serde_json::to_string(&close_msg) {
                                let mut channel = message_channel_clone.lock().await;
                                let _ = channel.send(Bytes::from(msg_json)).await;
                            }
                        });
                    }
                    Err(e) => {
                        error!("Error accepting connection on port {}: {}", local_port, e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop port forwarding for a local port
    pub async fn stop_port_forward(&self, local_port: u16) -> Result<(), ClientError> {
        // Ensure the message handler is started before sending requests
        self.ensure_message_handler_started().await;

        // Create the stop request
        let request = PortForwardMessage::StopRequest { local_port };

        // Serialize and send the request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| ClientError::Channel(format!("Failed to serialize request: {}", e)))?;

        // Send the request
        {
            let mut channel = self.message_channel.lock().await;
            channel
                .send(Bytes::from(request_json))
                .await
                .map_err(|e| ClientError::Channel(format!("Failed to send request: {}", e)))?;
        }

        // Wait for response on the control channel
        let response = {
            let mut control_rx = self.control_response_rx.lock().await;
            control_rx.recv().await.ok_or_else(|| {
                ClientError::Channel("Control response channel closed".to_string())
            })?
        };

        match response {
            PortForwardMessage::StopResponse { success: true, .. } => {
                // Clean up connections for this port
                {
                    let mut connections = self.active_connections.write().await;
                    connections.remove(&local_port);
                }

                // Stop local port forwarding
                self.port_forward_manager
                    .stop_forward(local_port)
                    .await
                    .map_err(|e| {
                        ClientError::Channel(format!("Failed to stop local forwarding: {}", e))
                    })?;

                info!("Port forwarding stopped for port {}", local_port);
                Ok(())
            }
            PortForwardMessage::StopResponse {
                success: false,
                error,
                ..
            } => Err(ClientError::RemoteExecution(error.unwrap_or_else(|| {
                "Unknown error stopping port forward".to_string()
            }))),
            _ => Err(ClientError::Channel("Unexpected response type".to_string())),
        }
    }

    /// Send a simple message (for testing compatibility)
    pub async fn send_message(&self, message: Bytes) -> Result<(), ClientError> {
        let mut channel = self.message_channel.lock().await;
        channel
            .send(message)
            .await
            .map_err(|e| ClientError::Channel(format!("Failed to send message: {}", e)))
    }

    /// Receive a simple message (for testing compatibility)
    pub async fn receive_message(&self) -> Result<Bytes, ClientError> {
        let mut channel = self.message_channel.lock().await;
        channel
            .receive()
            .await
            .map_err(|e| ClientError::Channel(format!("Failed to receive message: {}", e)))
    }

    /// Handle incoming port forward messages from remote
    pub async fn handle_incoming_message(
        &self,
        message: PortForwardMessage,
    ) -> Result<(), ClientError> {
        match message {
            PortForwardMessage::Data {
                local_port,
                connection_id,
                data,
            } => {
                let connections = self.active_connections.read().await;
                if let Some(port_connections) = connections.get(&local_port) {
                    if let Some(sender) = port_connections.get(&connection_id) {
                        if let Err(e) = sender.send(Bytes::from(data)) {
                            warn!("Failed to send data to connection {}: {}", connection_id, e);
                        }
                    }
                }
            }
            PortForwardMessage::CloseConnection {
                local_port,
                connection_id,
            } => {
                let mut connections = self.active_connections.write().await;
                if let Some(port_connections) = connections.get_mut(&local_port) {
                    port_connections.remove(&connection_id);
                }
            }
            _ => {
                debug!("Ignoring message type: {:?}", message);
            }
        }
        Ok(())
    }
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

        // Create a channel
        let channel = handle.channel_open_session().await?;
        let channel_id = channel.id();
        debug!("Created SSH channel: {:?}", channel_id);

        // Execute the remote command
        let remote_path = "/usr/local/bin/yuha-remote";
        let command = format!("{} -s 2>/tmp/remote_stderr.log", remote_path);
        debug!("Executing command: {}", command);

        // Execute the command using channel exec
        channel.exec(false, command.as_bytes()).await?;
        debug!("Command executed successfully");

        // Create the adapter
        let ssh_adapter = SshChannelAdapter::new(handle, channel_id, data_rx);

        // Create a message channel using binary mode
        let message_channel = MessageChannel::new_with_stream(ssh_adapter);

        Ok(message_channel)
    }

    /// Connect to a remote host via SSH and return a YuhaClient
    pub async fn connect_ssh_with_port_forward(
        host: &str,
        port: u16,
        username: &str,
        password: Option<&str>,
        key_path: Option<&Path>,
    ) -> Result<YuhaClient, ClientError> {
        let message_channel = connect_ssh(host, port, username, password, key_path).await?;
        Ok(YuhaClient::new(message_channel))
    }

    /// Get the path to the built remote binary
    pub fn get_remote_binary_path() -> &'static str {
        REMOTE_BINARY_PATH
    }
}
