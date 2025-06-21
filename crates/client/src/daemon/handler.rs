//! Request handler for daemon operations
//!
//! This module handles incoming requests from CLI clients and manages
//! the session lifecycle.

use crate::daemon_protocol::{
    CommandResult, DaemonCommand, DaemonRequest, DaemonResponse, ErrorCode, SessionDetails,
    SessionSummary,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use yuha_core::{
    TransportFactory,
    session::{SessionId, SessionManager, SessionStatus},
};

/// Active client connections mapped by session ID
type ClientMap = Arc<Mutex<HashMap<SessionId, Arc<Mutex<Box<dyn std::any::Any + Send>>>>>>;

/// Request handler for processing daemon requests
pub struct RequestHandler {
    session_manager: Arc<SessionManager>,
    active_clients: ClientMap,
}

impl RequestHandler {
    /// Create a new request handler
    pub fn new(session_manager: Arc<SessionManager>) -> Self {
        Self {
            session_manager,
            active_clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Handle an incoming request
    pub async fn handle_request(&self, request: DaemonRequest) -> DaemonResponse {
        match request {
            DaemonRequest::Ping => DaemonResponse::Pong,

            DaemonRequest::CreateSession {
                name,
                transport_config,
                tags,
                description,
            } => {
                self.handle_create_session(name, transport_config, tags, description)
                    .await
            }

            DaemonRequest::ConnectSession {
                name,
                transport_config,
            } => self.handle_connect_session(name, transport_config).await,

            DaemonRequest::DisconnectSession { session_id } => {
                self.handle_disconnect_session(session_id).await
            }

            DaemonRequest::ListSessions => self.handle_list_sessions().await,

            DaemonRequest::GetSessionInfo { session_id } => {
                self.handle_get_session_info(session_id).await
            }

            DaemonRequest::ExecuteCommand {
                session_id,
                command,
            } => self.handle_execute_command(session_id, command).await,

            DaemonRequest::Shutdown => {
                // Shutdown is handled by the server
                DaemonResponse::ShuttingDown
            }
        }
    }

    /// Handle session creation
    async fn handle_create_session(
        &self,
        name: String,
        transport_config: yuha_core::transport::TransportConfig,
        tags: Vec<String>,
        description: Option<String>,
    ) -> DaemonResponse {
        // Check if we can reuse an existing session
        let connection_key = transport_config.connection_key();
        if let Some(session_id) = self
            .session_manager
            .find_pooled_session(&connection_key)
            .await
        {
            // Mark session as used
            if self.session_manager.use_session(session_id).await.is_ok() {
                info!("Reusing existing session {} for {}", session_id, name);
                return DaemonResponse::SessionCreated {
                    session_id,
                    reused: true,
                };
            }
        }

        // Create new session
        match self
            .session_manager
            .create_session(name.clone(), transport_config.clone(), tags, description)
            .await
        {
            Ok(session_id) => {
                // Create the actual client connection
                if let Err(e) = self.connect_client(session_id, transport_config).await {
                    error!("Failed to connect client for session {}: {}", session_id, e);
                    let _ = self.session_manager.close_session(session_id).await;
                    return DaemonResponse::Error {
                        code: ErrorCode::ConnectionFailed,
                        message: format!("Failed to establish connection: {}", e),
                    };
                }

                info!("Created new session {} for {}", session_id, name);
                DaemonResponse::SessionCreated {
                    session_id,
                    reused: false,
                }
            }
            Err(e) => {
                error!("Failed to create session: {}", e);
                DaemonResponse::Error {
                    code: ErrorCode::MaxSessionsReached,
                    message: e.to_string(),
                }
            }
        }
    }

    /// Handle connect to existing or new session
    async fn handle_connect_session(
        &self,
        name: String,
        transport_config: yuha_core::transport::TransportConfig,
    ) -> DaemonResponse {
        // Try to find existing session by name
        let sessions = self.session_manager.list_sessions().await;
        for session in sessions {
            if session.name == name && session.status == SessionStatus::Active {
                // Reuse existing session
                if self.session_manager.use_session(session.id).await.is_ok() {
                    return DaemonResponse::SessionCreated {
                        session_id: session.id,
                        reused: true,
                    };
                }
            }
        }

        // Create new session if not found
        self.handle_create_session(name, transport_config, vec![], None)
            .await
    }

    /// Handle session disconnection
    async fn handle_disconnect_session(&self, session_id: SessionId) -> DaemonResponse {
        // Remove active client
        self.active_clients.lock().await.remove(&session_id);

        // Update session status to idle (not closing it for pooling)
        match self
            .session_manager
            .update_session_status(session_id, SessionStatus::Idle)
            .await
        {
            Ok(()) => {
                info!("Session {} marked as idle", session_id);
                DaemonResponse::SessionDisconnected
            }
            Err(e) => {
                error!("Failed to disconnect session {}: {}", session_id, e);
                DaemonResponse::Error {
                    code: ErrorCode::SessionNotFound,
                    message: e.to_string(),
                }
            }
        }
    }

    /// Handle list sessions request
    async fn handle_list_sessions(&self) -> DaemonResponse {
        let sessions = self.session_manager.list_sessions().await;
        let summaries: Vec<SessionSummary> = sessions
            .into_iter()
            .map(|metadata| {
                let connection_key = metadata.connection_key();
                SessionSummary {
                    id: metadata.id,
                    name: metadata.name,
                    connection_key,
                    status: format!("{:?}", metadata.status),
                    created_at: format!("{:?}", metadata.created_at),
                    last_used: format!("{:?}", metadata.last_used),
                    use_count: metadata.use_count,
                }
            })
            .collect();

        DaemonResponse::SessionList {
            sessions: summaries,
        }
    }

    /// Handle get session info request
    async fn handle_get_session_info(&self, session_id: SessionId) -> DaemonResponse {
        match self.session_manager.get_session_metadata(session_id).await {
            Some(metadata) => {
                let details = SessionDetails {
                    id: metadata.id,
                    name: metadata.name,
                    transport_config: metadata.transport_config,
                    status: format!("{:?}", metadata.status),
                    created_at: format!("{:?}", metadata.created_at),
                    last_used: format!("{:?}", metadata.last_used),
                    use_count: metadata.use_count,
                    tags: metadata.tags,
                    description: metadata.description,
                    active_port_forwards: vec![], // TODO: Track port forwards
                };

                DaemonResponse::SessionInfo {
                    session: Box::new(details),
                }
            }
            None => DaemonResponse::Error {
                code: ErrorCode::SessionNotFound,
                message: format!("Session {} not found", session_id),
            },
        }
    }

    /// Handle command execution
    async fn handle_execute_command(
        &self,
        session_id: SessionId,
        command: DaemonCommand,
    ) -> DaemonResponse {
        // Get the client for this session
        let clients = self.active_clients.lock().await;
        let client = match clients.get(&session_id) {
            Some(client) => client.clone(),
            None => {
                return DaemonResponse::Error {
                    code: ErrorCode::SessionNotFound,
                    message: format!("Session {} not connected", session_id),
                };
            }
        };
        drop(clients);

        // Mark session as used
        if let Err(e) = self.session_manager.use_session(session_id).await {
            warn!("Failed to mark session {} as used: {}", session_id, e);
        }

        // Execute command based on transport type
        match self.execute_command_on_client(client, command).await {
            Ok(result) => DaemonResponse::CommandSuccess { result },
            Err(e) => {
                error!("Command execution failed for session {}: {}", session_id, e);
                DaemonResponse::Error {
                    code: ErrorCode::CommandFailed,
                    message: e.to_string(),
                }
            }
        }
    }

    /// Connect a client for a session
    async fn connect_client(
        &self,
        session_id: SessionId,
        transport_config: yuha_core::transport::TransportConfig,
    ) -> anyhow::Result<()> {
        // Update session status
        self.session_manager
            .update_session_status(session_id, SessionStatus::Connecting)
            .await?;

        // Create transport and connect
        let _transport = TransportFactory::create(transport_config)?;

        // This is a simplified version - in reality, we'd need to handle different transport types
        // For now, we'll store a placeholder
        let client: Arc<Mutex<Box<dyn std::any::Any + Send>>> = Arc::new(Mutex::new(Box::new(())));

        self.active_clients.lock().await.insert(session_id, client);

        // Update session status to active
        self.session_manager
            .update_session_status(session_id, SessionStatus::Active)
            .await?;

        Ok(())
    }

    /// Execute a command on the client
    async fn execute_command_on_client(
        &self,
        _client: Arc<Mutex<Box<dyn std::any::Any + Send>>>,
        command: DaemonCommand,
    ) -> anyhow::Result<CommandResult> {
        // This is a placeholder implementation
        // In reality, we'd downcast the client to the appropriate type and execute the command
        match command {
            DaemonCommand::GetClipboard => Ok(CommandResult::ClipboardContent {
                content: "Clipboard content placeholder".to_string(),
            }),
            DaemonCommand::SetClipboard { content } => {
                debug!("Setting clipboard to: {}", content);
                Ok(CommandResult::ClipboardSet)
            }
            DaemonCommand::OpenBrowser { url } => {
                debug!("Opening browser to: {}", url);
                Ok(CommandResult::BrowserOpened)
            }
            DaemonCommand::StartPortForward {
                local_port,
                remote_host,
                remote_port,
            } => {
                debug!(
                    "Starting port forward: {} -> {}:{}",
                    local_port, remote_host, remote_port
                );
                Ok(CommandResult::PortForwardStarted)
            }
            DaemonCommand::StopPortForward { local_port } => {
                debug!("Stopping port forward on port {}", local_port);
                Ok(CommandResult::PortForwardStopped)
            }
            DaemonCommand::PortForwardData {
                connection_id,
                data,
            } => {
                debug!(
                    "Forwarding {} bytes for connection {}",
                    data.len(),
                    connection_id
                );
                Ok(CommandResult::PortForwardDataSent)
            }
        }
    }
}
