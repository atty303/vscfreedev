//! Modular session management system
//!
//! This module provides a clean separation of concerns for session management,
//! breaking down the monolithic SessionManager into focused components.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

pub mod lifecycle;
pub mod manager;
pub mod metrics;
pub mod pool;
pub mod registry;

// Re-export commonly used types
pub use lifecycle::{SessionLifecycle, SessionLifecycleConfig};
pub use manager::SessionManager;
pub use metrics::{SessionMetrics, SessionMetricsCollector};
pub use pool::{SessionPool, SessionPoolConfig};
pub use registry::{SessionRegistry, SessionRegistryConfig};

/// Unique identifier for a session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    /// Create a new random session ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Get the string representation of the session ID
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }

    /// Parse a session ID from a string
    pub fn parse<S: AsRef<str>>(s: S) -> Result<Self> {
        let uuid = Uuid::parse_str(s.as_ref())
            .map_err(|e| crate::error::YuhaError::session(format!("Invalid session ID: {}", e)))?;
        Ok(Self(uuid))
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SessionId {
    type Err = crate::error::YuhaError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

/// Status of a session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    /// Session is being created
    Creating,
    /// Session is connecting
    Connecting,
    /// Session is active and ready
    Active,
    /// Session is idle (connected but not being used)
    Idle,
    /// Session is being reconnected
    Reconnecting,
    /// Session has failed
    Failed,
    /// Session is being closed
    Closing,
    /// Session is closed
    Closed,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Creating => write!(f, "creating"),
            SessionStatus::Connecting => write!(f, "connecting"),
            SessionStatus::Active => write!(f, "active"),
            SessionStatus::Idle => write!(f, "idle"),
            SessionStatus::Reconnecting => write!(f, "reconnecting"),
            SessionStatus::Failed => write!(f, "failed"),
            SessionStatus::Closing => write!(f, "closing"),
            SessionStatus::Closed => write!(f, "closed"),
        }
    }
}

/// Priority level for sessions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum SessionPriority {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

/// Metadata about a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Unique session identifier
    pub id: SessionId,
    /// Human-readable name for the session
    pub name: String,
    /// Transport configuration used for this session
    pub transport_config: crate::transport::TransportConfig,
    /// Current status of the session
    pub status: SessionStatus,
    /// Session priority
    pub priority: SessionPriority,
    /// When the session was created
    pub created_at: SystemTime,
    /// When the session was last used
    pub last_used: SystemTime,
    /// Number of times this session has been used
    pub use_count: u64,
    /// Optional tags for categorizing sessions
    pub tags: Vec<String>,
    /// Optional description
    pub description: Option<String>,
}

impl SessionMetadata {
    /// Create new session metadata
    pub fn new<S: Into<String>>(
        name: S,
        transport_config: crate::transport::TransportConfig,
        tags: Vec<String>,
        description: Option<String>,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            id: SessionId::new(),
            name: name.into(),
            transport_config,
            status: SessionStatus::Creating,
            priority: SessionPriority::default(),
            created_at: now,
            last_used: now,
            use_count: 0,
            tags,
            description,
        }
    }

    /// Update the last used timestamp and increment use count
    pub fn mark_used(&mut self) {
        self.last_used = SystemTime::now();
        self.use_count += 1;
    }

    /// Set the session status
    pub fn set_status(&mut self, status: SessionStatus) {
        self.status = status;
        if matches!(status, SessionStatus::Active | SessionStatus::Idle) {
            self.last_used = SystemTime::now();
        }
    }

    /// Set the session priority
    pub fn set_priority(&mut self, priority: SessionPriority) {
        self.priority = priority;
    }

    /// Get the age of the session
    pub fn age(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.created_at)
            .unwrap_or(Duration::ZERO)
    }

    /// Get the idle time of the session
    pub fn idle_time(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.last_used)
            .unwrap_or(Duration::ZERO)
    }

    /// Check if the session matches given tags
    pub fn has_tags(&self, tags: &[String]) -> bool {
        tags.iter().all(|tag| self.tags.contains(tag))
    }

    /// Check if the session has any of the given tags
    pub fn has_any_tag(&self, tags: &[String]) -> bool {
        tags.iter().any(|tag| self.tags.contains(tag))
    }

    /// Generate a connection key for grouping similar connections
    pub fn connection_key(&self) -> String {
        self.transport_config.connection_key()
    }

    /// Check if the session is usable
    pub fn is_usable(&self) -> bool {
        matches!(self.status, SessionStatus::Active | SessionStatus::Idle)
    }

    /// Check if the session is failed or closed
    pub fn is_terminated(&self) -> bool {
        matches!(self.status, SessionStatus::Failed | SessionStatus::Closed)
    }

    /// Check if the session is in a transitional state
    pub fn is_transitional(&self) -> bool {
        matches!(
            self.status,
            SessionStatus::Creating
                | SessionStatus::Connecting
                | SessionStatus::Reconnecting
                | SessionStatus::Closing
        )
    }
}

/// Session statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStats {
    /// Total number of sessions created
    pub total_sessions_created: u64,
    /// Number of currently active sessions
    pub active_sessions: u32,
    /// Number of idle sessions
    pub idle_sessions: u32,
    /// Number of failed sessions
    pub failed_sessions: u32,
    /// Total connection attempts
    pub total_connections: u64,
    /// Successful connections
    pub successful_connections: u64,
    /// Failed connections
    pub failed_connections: u64,
    /// Average session lifetime
    pub avg_session_lifetime: Duration,
    /// Session usage by transport type
    pub transport_usage: std::collections::HashMap<crate::transport::TransportType, u32>,
}

/// Configuration for the overall session management system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionManagerConfig {
    /// Registry configuration
    pub registry: SessionRegistryConfig,
    /// Pool configuration
    pub pool: SessionPoolConfig,
    /// Lifecycle management configuration
    pub lifecycle: SessionLifecycleConfig,
    /// Enable detailed metrics collection
    #[serde(default)]
    pub enable_metrics: bool,
}

impl Default for SessionManagerConfig {
    fn default() -> Self {
        Self {
            registry: SessionRegistryConfig::default(),
            pool: SessionPoolConfig::default(),
            lifecycle: SessionLifecycleConfig::default(),
            enable_metrics: true,
        }
    }
}
