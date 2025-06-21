//! Daemon server implementation
//!
//! This module implements the main daemon server that listens for client
//! connections and handles requests.

use crate::daemon_protocol::{DaemonConfig, DaemonRequest, DaemonResponse};
use anyhow::{Context, Result};
use bytes::Bytes;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixListener;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use yuha_core::{
    message_channel::MessageChannel,
    session::{SessionManager, SessionManagerConfig},
};

use super::handler::RequestHandler;

/// Daemon server that listens for client connections
pub struct DaemonServer {
    config: DaemonConfig,
    session_manager: Arc<SessionManager>,
    request_handler: Arc<RequestHandler>,
    shutdown: Arc<RwLock<bool>>,
}

impl DaemonServer {
    /// Create a new daemon server
    pub fn new(config: DaemonConfig) -> Self {
        // Create session manager with configuration
        let session_config = SessionManagerConfig {
            max_concurrent_sessions: 50,
            max_sessions_per_connection: 5,
            max_idle_time: std::time::Duration::from_secs(config.session_idle_timeout),
            enable_pooling: config.enable_session_pooling,
            ..Default::default()
        };

        let session_manager = Arc::new(SessionManager::new(session_config));
        let request_handler = Arc::new(RequestHandler::new(Arc::clone(&session_manager)));

        Self {
            config,
            session_manager,
            request_handler,
            shutdown: Arc::new(RwLock::new(false)),
        }
    }

    /// Run the daemon server
    pub async fn run(self) -> Result<()> {
        info!("Starting daemon server on {}", self.config.socket_path);

        // Start session cleanup task
        self.session_manager.start_cleanup_task().await?;

        // Clean up any existing socket file
        #[cfg(unix)]
        {
            let socket_path = std::path::Path::new(&self.config.socket_path);
            if socket_path.exists() {
                std::fs::remove_file(socket_path).with_context(|| {
                    format!("Failed to remove existing socket file: {:?}", socket_path)
                })?;
            }
        }

        // Create listener
        #[cfg(unix)]
        let listener = UnixListener::bind(&self.config.socket_path)
            .with_context(|| format!("Failed to bind to socket: {}", self.config.socket_path))?;

        #[cfg(windows)]
        let listener = {
            use tokio::net::windows::named_pipe::ServerOptions;
            ServerOptions::new()
                .first_pipe_instance(true)
                .create(&self.config.socket_path)
                .with_context(|| {
                    format!("Failed to create named pipe: {}", self.config.socket_path)
                })?
        };

        info!("Daemon server listening on {}", self.config.socket_path);

        // Accept connections
        let mut client_count = 0u32;
        loop {
            // Check for shutdown
            if *self.shutdown.read().await {
                info!("Daemon server shutting down");
                break;
            }

            // Accept new connection
            #[cfg(unix)]
            let (stream, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    continue;
                }
            };

            #[cfg(windows)]
            let stream = {
                // Windows named pipe handling would go here
                // For now, we'll use a placeholder
                todo!("Windows named pipe support not yet implemented")
            };

            client_count += 1;
            let client_id = client_count;

            // Check max clients
            if client_count > self.config.max_clients {
                warn!(
                    "Maximum client connections reached ({})",
                    self.config.max_clients
                );
                continue;
            }

            info!("Client {} connected", client_id);

            // Handle client in a separate task
            let handler = Arc::clone(&self.request_handler);
            let shutdown = Arc::clone(&self.shutdown);

            tokio::spawn(async move {
                if let Err(e) = handle_client(client_id, stream, handler, shutdown).await {
                    error!("Error handling client {}: {}", client_id, e);
                }
                info!("Client {} disconnected", client_id);
            });
        }

        // Stop cleanup task
        self.session_manager.stop_cleanup_task().await;

        // Clean up socket file
        #[cfg(unix)]
        {
            let socket_path = std::path::Path::new(&self.config.socket_path);
            if socket_path.exists() {
                let _ = std::fs::remove_file(socket_path);
            }
        }

        Ok(())
    }
}

/// Handle a single client connection
async fn handle_client<S>(
    client_id: u32,
    stream: S,
    handler: Arc<RequestHandler>,
    shutdown: Arc<RwLock<bool>>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut channel = MessageChannel::<S>::new_with_stream(stream);

    loop {
        // Receive request
        let request_bytes = match channel.receive().await {
            Ok(bytes) => bytes,
            Err(e) => {
                debug!("Client {} disconnected: {}", client_id, e);
                break;
            }
        };

        // Deserialize request
        let request: DaemonRequest = match serde_json::from_slice(&request_bytes) {
            Ok(req) => req,
            Err(e) => {
                error!(
                    "Failed to deserialize request from client {}: {}",
                    client_id, e
                );
                continue;
            }
        };

        debug!("Client {} request: {:?}", client_id, request);

        // Handle special shutdown request
        if matches!(request, DaemonRequest::Shutdown) {
            info!("Shutdown requested by client {}", client_id);
            *shutdown.write().await = true;
            let response = DaemonResponse::ShuttingDown;
            let response_bytes = Bytes::from(serde_json::to_vec(&response).unwrap());
            let _ = channel.send(response_bytes).await;
            break;
        }

        // Process request
        let response = handler.handle_request(request).await;

        // Serialize response
        let response_bytes = match serde_json::to_vec(&response) {
            Ok(bytes) => Bytes::from(bytes),
            Err(e) => {
                error!(
                    "Failed to serialize response for client {}: {}",
                    client_id, e
                );
                continue;
            }
        };

        // Send response
        if let Err(e) = channel.send(response_bytes).await {
            error!("Failed to send response to client {}: {}", client_id, e);
            break;
        }
    }

    Ok(())
}
