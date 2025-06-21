//! Daemon protocol implementation for CLI-daemon communication

use crate::session::SessionId;
use crate::transport::TransportConfig;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Daemon protocol request types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonRequest {
    /// Check if daemon is alive
    Ping,

    /// Create a new session with the given configuration
    CreateSession {
        name: String,
        transport_config: TransportConfig,
        tags: Vec<String>,
        description: Option<String>,
    },

    /// Connect to an existing session or create if not exists
    ConnectSession {
        name: String,
        transport_config: TransportConfig,
    },

    /// Disconnect from a session
    DisconnectSession { session_id: SessionId },

    /// List all active sessions
    ListSessions,

    /// Get detailed information about a specific session
    GetSessionInfo { session_id: SessionId },

    /// Execute a command on a session
    ExecuteCommand {
        session_id: SessionId,
        command: DaemonCommand,
    },

    /// Shutdown the daemon
    Shutdown,
}

/// Commands that can be executed on a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonCommand {
    /// Get clipboard content
    GetClipboard,

    /// Set clipboard content
    SetClipboard { content: String },

    /// Open browser with URL
    OpenBrowser { url: String },

    /// Start port forwarding
    StartPortForward {
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    },

    /// Stop port forwarding
    StopPortForward { local_port: u16 },

    /// Send port forward data
    PortForwardData { connection_id: u32, data: Bytes },
}

/// Daemon protocol response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonResponse {
    /// Pong response to ping
    Pong,

    /// Session created/connected successfully
    SessionCreated { session_id: SessionId, reused: bool },

    /// Session disconnected successfully
    SessionDisconnected,

    /// List of active sessions
    SessionList { sessions: Vec<SessionSummary> },

    /// Detailed session information
    SessionInfo { session: Box<SessionDetails> },

    /// Command executed successfully
    CommandSuccess { result: CommandResult },

    /// Daemon shutting down
    ShuttingDown,

    /// Error occurred
    Error { code: ErrorCode, message: String },
}

/// Summary information about a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: SessionId,
    pub name: String,
    pub connection_key: String,
    pub status: String,
    pub created_at: String,
    pub last_used: String,
    pub use_count: u64,
}

/// Detailed information about a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetails {
    pub id: SessionId,
    pub name: String,
    pub transport_config: TransportConfig,
    pub status: String,
    pub created_at: String,
    pub last_used: String,
    pub use_count: u64,
    pub tags: Vec<String>,
    pub description: Option<String>,
    pub active_port_forwards: Vec<PortForwardInfo>,
}

/// Information about active port forwarding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForwardInfo {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub active_connections: u32,
}

/// Result of command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandResult {
    /// Clipboard content retrieved
    ClipboardContent { content: String },

    /// Clipboard set successfully
    ClipboardSet,

    /// Browser opened successfully
    BrowserOpened,

    /// Port forward started
    PortForwardStarted,

    /// Port forward stopped
    PortForwardStopped,

    /// Port forward data sent
    PortForwardDataSent,
}

/// Error codes for daemon responses
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    /// Session not found
    SessionNotFound,

    /// Session already exists
    SessionAlreadyExists,

    /// Connection failed
    ConnectionFailed,

    /// Command execution failed
    CommandFailed,

    /// Invalid request
    InvalidRequest,

    /// Daemon shutting down
    ShuttingDown,

    /// Internal error
    InternalError,

    /// Maximum sessions reached
    MaxSessionsReached,

    /// Authentication failed
    AuthenticationFailed,
}

/// Daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Socket path for Unix domain socket (Unix) or pipe name (Windows)
    pub socket_path: String,

    /// Maximum number of concurrent client connections
    pub max_clients: u32,

    /// Enable session pooling
    pub enable_session_pooling: bool,

    /// Session idle timeout in seconds
    pub session_idle_timeout: u64,

    /// Log file path (optional)
    pub log_file: Option<String>,

    /// PID file path (optional)
    pub pid_file: Option<String>,

    /// Additional configuration options
    pub options: HashMap<String, String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: "/tmp/yuha.sock".to_string(),
            max_clients: 10,
            enable_session_pooling: true,
            session_idle_timeout: 300, // 5 minutes
            log_file: None,
            pid_file: None,
            options: HashMap::new(),
        }
    }
}
