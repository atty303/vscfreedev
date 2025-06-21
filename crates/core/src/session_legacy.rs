//! Session management for multiple connections
//!
//! This module provides session management capabilities for handling multiple
//! simultaneous connections to different servers with connection pooling,
//! lifecycle management, and resource limits.

use crate::error::{Result, YuhaError};
use crate::transport::{TransportConfig, TransportType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

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
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Status of a session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
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

/// Metadata about a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Unique session identifier
    pub id: SessionId,
    /// Human-readable name for the session
    pub name: String,
    /// Transport configuration used for this session
    pub transport_config: TransportConfig,
    /// Current status of the session
    pub status: SessionStatus,
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
    pub fn new(
        name: String,
        transport_config: TransportConfig,
        tags: Vec<String>,
        description: Option<String>,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            id: SessionId::new(),
            name,
            transport_config,
            status: SessionStatus::Connecting,
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

    /// Generate a connection key for grouping similar connections
    pub fn connection_key(&self) -> String {
        match &self.transport_config.transport_type {
            TransportType::Ssh => {
                if let Some(ssh) = &self.transport_config.ssh {
                    format!("ssh://{}@{}:{}", ssh.username, ssh.host, ssh.port)
                } else {
                    "ssh://unknown".to_string()
                }
            }
            TransportType::Local => {
                if let Some(local) = &self.transport_config.local {
                    format!("local://{}", local.binary_path.display())
                } else {
                    "local://unknown".to_string()
                }
            }
            TransportType::Tcp => {
                if let Some(tcp) = &self.transport_config.tcp {
                    format!("tcp://{}:{}", tcp.host, tcp.port)
                } else {
                    "tcp://unknown".to_string()
                }
            }
            TransportType::Wsl => {
                if let Some(wsl) = &self.transport_config.wsl {
                    format!(
                        "wsl://{}@{}",
                        wsl.user.as_deref().unwrap_or("default"),
                        wsl.distribution.as_deref().unwrap_or("default")
                    )
                } else {
                    "wsl://default".to_string()
                }
            }
        }
    }
}

/// Configuration for session management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionManagerConfig {
    /// Maximum number of concurrent sessions
    pub max_concurrent_sessions: u32,
    /// Maximum number of sessions per connection key
    pub max_sessions_per_connection: u32,
    /// Maximum idle time before a session is automatically closed
    pub max_idle_time: Duration,
    /// Maximum session lifetime
    pub max_session_lifetime: Duration,
    /// Interval for cleanup operations
    pub cleanup_interval: Duration,
    /// Enable connection pooling
    pub enable_pooling: bool,
    /// Pool size per connection key
    pub pool_size_per_connection: u32,
    /// Enable automatic reconnection
    pub enable_auto_reconnect: bool,
    /// Maximum reconnection attempts
    pub max_reconnect_attempts: u32,
    /// Delay between reconnection attempts
    pub reconnect_delay: Duration,
}

impl Default for SessionManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_sessions: 50,
            max_sessions_per_connection: 5,
            max_idle_time: Duration::from_secs(300), // 5 minutes
            max_session_lifetime: Duration::from_secs(3600), // 1 hour
            cleanup_interval: Duration::from_secs(60), // 1 minute
            enable_pooling: true,
            pool_size_per_connection: 2,
            enable_auto_reconnect: true,
            max_reconnect_attempts: 3,
            reconnect_delay: Duration::from_secs(5),
        }
    }
}

/// Statistics about session usage
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub transport_usage: HashMap<TransportType, u32>,
}

impl Default for SessionStats {
    fn default() -> Self {
        Self {
            total_sessions_created: 0,
            active_sessions: 0,
            idle_sessions: 0,
            failed_sessions: 0,
            total_connections: 0,
            successful_connections: 0,
            failed_connections: 0,
            avg_session_lifetime: Duration::ZERO,
            transport_usage: HashMap::new(),
        }
    }
}

/// Session manager for handling multiple connections
pub struct SessionManager {
    /// Configuration for the session manager
    config: SessionManagerConfig,
    /// Map of session ID to session metadata
    sessions: Arc<RwLock<HashMap<SessionId, SessionMetadata>>>,
    /// Map of connection keys to session pools
    connection_pools: Arc<RwLock<HashMap<String, Vec<SessionId>>>>,
    /// Global statistics
    stats: Arc<Mutex<SessionStats>>,
    /// Atomic counter for session creation
    session_counter: AtomicU64,
    /// Flag to indicate if cleanup task is running
    cleanup_running: Arc<Mutex<bool>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(config: SessionManagerConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            connection_pools: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(Mutex::new(SessionStats::default())),
            session_counter: AtomicU64::new(0),
            cleanup_running: Arc::new(Mutex::new(false)),
        }
    }

    /// Create a new session with the given configuration
    pub async fn create_session(
        &self,
        name: String,
        transport_config: TransportConfig,
        tags: Vec<String>,
        description: Option<String>,
    ) -> Result<SessionId> {
        // Check if we've reached the maximum number of concurrent sessions
        let sessions = self.sessions.read().await;
        if sessions.len() >= self.config.max_concurrent_sessions as usize {
            return Err(YuhaError::session("Maximum concurrent sessions reached"));
        }

        // Create session metadata
        let metadata = SessionMetadata::new(name.clone(), transport_config, tags, description);
        let session_id = metadata.id;
        let connection_key = metadata.connection_key();

        // Check per-connection limits
        let pools = self.connection_pools.read().await;
        if let Some(pool) = pools.get(&connection_key) {
            let active_count = pool.len();
            if active_count >= self.config.max_sessions_per_connection as usize {
                return Err(YuhaError::session(format!(
                    "Maximum sessions per connection reached for {}",
                    connection_key
                )));
            }
        }
        drop(pools);
        drop(sessions);

        // Add session to the manager
        self.sessions.write().await.insert(session_id, metadata);

        // Add to connection pool
        self.connection_pools
            .write()
            .await
            .entry(connection_key)
            .or_default()
            .push(session_id);

        // Update statistics
        let mut stats = self.stats.lock().await;
        stats.total_sessions_created += 1;
        stats.active_sessions += 1;
        self.session_counter.fetch_add(1, Ordering::Relaxed);

        info!("Created session {} ({})", session_id, name);
        debug!(
            "Session metadata: {:?}",
            self.get_session_metadata(session_id).await
        );

        Ok(session_id)
    }

    /// Get session metadata by ID
    pub async fn get_session_metadata(&self, session_id: SessionId) -> Option<SessionMetadata> {
        self.sessions.read().await.get(&session_id).cloned()
    }

    /// Update session status
    pub async fn update_session_status(
        &self,
        session_id: SessionId,
        status: SessionStatus,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(metadata) = sessions.get_mut(&session_id) {
            let old_status = metadata.status;
            metadata.status = status;

            debug!(
                "Session {} status changed: {:?} -> {:?}",
                session_id, old_status, status
            );

            // Update statistics
            let mut stats = self.stats.lock().await;
            match (old_status, status) {
                (SessionStatus::Connecting, SessionStatus::Active) => {
                    stats.successful_connections += 1;
                }
                (_, SessionStatus::Failed) => {
                    stats.failed_connections += 1;
                    stats.failed_sessions += 1;
                    stats.active_sessions = stats.active_sessions.saturating_sub(1);
                }
                (_, SessionStatus::Closed) => {
                    stats.active_sessions = stats.active_sessions.saturating_sub(1);
                }
                (SessionStatus::Active, SessionStatus::Idle) => {
                    stats.idle_sessions += 1;
                }
                (SessionStatus::Idle, SessionStatus::Active) => {
                    stats.idle_sessions = stats.idle_sessions.saturating_sub(1);
                }
                _ => {}
            }

            Ok(())
        } else {
            Err(YuhaError::session(format!(
                "Session {} not found",
                session_id
            )))
        }
    }

    /// Mark a session as used
    pub async fn use_session(&self, session_id: SessionId) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(metadata) = sessions.get_mut(&session_id) {
            metadata.mark_used();

            // If session was idle, mark it as active
            if metadata.status == SessionStatus::Idle {
                metadata.status = SessionStatus::Active;
                let mut stats = self.stats.lock().await;
                stats.idle_sessions = stats.idle_sessions.saturating_sub(1);
            }

            debug!(
                "Session {} marked as used (count: {})",
                session_id, metadata.use_count
            );
            Ok(())
        } else {
            Err(YuhaError::session(format!(
                "Session {} not found",
                session_id
            )))
        }
    }

    /// Close a session
    pub async fn close_session(&self, session_id: SessionId) -> Result<()> {
        info!("Closing session {}", session_id);

        // Update status to closing
        self.update_session_status(session_id, SessionStatus::Closing)
            .await?;

        // Remove from connection pools
        let mut pools = self.connection_pools.write().await;
        for pool in pools.values_mut() {
            pool.retain(|&id| id != session_id);
        }
        drop(pools);

        // Remove from sessions and update status
        let mut sessions = self.sessions.write().await;
        if let Some(mut metadata) = sessions.remove(&session_id) {
            metadata.status = SessionStatus::Closed;
            debug!(
                "Session {} closed after {} uses",
                session_id, metadata.use_count
            );
            Ok(())
        } else {
            Err(YuhaError::session(format!(
                "Session {} not found",
                session_id
            )))
        }
    }

    /// List all sessions
    pub async fn list_sessions(&self) -> Vec<SessionMetadata> {
        self.sessions.read().await.values().cloned().collect()
    }

    /// List sessions by tags
    pub async fn list_sessions_by_tags(&self, tags: &[String]) -> Vec<SessionMetadata> {
        self.sessions
            .read()
            .await
            .values()
            .filter(|metadata| metadata.has_tags(tags))
            .cloned()
            .collect()
    }

    /// Find available session in pool for a given connection key
    pub async fn find_pooled_session(&self, connection_key: &str) -> Option<SessionId> {
        let pools = self.connection_pools.read().await;
        let sessions = self.sessions.read().await;

        if let Some(pool) = pools.get(connection_key) {
            // Find an idle session in the pool
            for &session_id in pool {
                if let Some(metadata) = sessions.get(&session_id) {
                    if metadata.status == SessionStatus::Idle {
                        return Some(session_id);
                    }
                }
            }
        }

        None
    }

    /// Get session statistics
    pub async fn get_stats(&self) -> SessionStats {
        self.stats.lock().await.clone()
    }

    /// Start the cleanup task
    pub async fn start_cleanup_task(&self) -> Result<()> {
        let mut cleanup_running = self.cleanup_running.lock().await;
        if *cleanup_running {
            return Ok(());
        }
        *cleanup_running = true;
        drop(cleanup_running);

        let sessions = Arc::clone(&self.sessions);
        let connection_pools = Arc::clone(&self.connection_pools);
        let stats = Arc::clone(&self.stats);
        let cleanup_running = Arc::clone(&self.cleanup_running);
        let config = self.config.clone();

        tokio::spawn(async move {
            info!("Starting session cleanup task");
            let mut interval = tokio::time::interval(config.cleanup_interval);

            loop {
                interval.tick().await;

                if !*cleanup_running.lock().await {
                    break;
                }

                // Perform cleanup
                let expired_sessions =
                    Self::cleanup_expired_sessions(&sessions, &connection_pools, &stats, &config)
                        .await;

                if !expired_sessions.is_empty() {
                    info!("Cleaned up {} expired sessions", expired_sessions.len());
                }
            }

            info!("Session cleanup task stopped");
        });

        Ok(())
    }

    /// Stop the cleanup task
    pub async fn stop_cleanup_task(&self) {
        *self.cleanup_running.lock().await = false;
    }

    /// Internal cleanup function
    async fn cleanup_expired_sessions(
        sessions: &Arc<RwLock<HashMap<SessionId, SessionMetadata>>>,
        connection_pools: &Arc<RwLock<HashMap<String, Vec<SessionId>>>>,
        stats: &Arc<Mutex<SessionStats>>,
        config: &SessionManagerConfig,
    ) -> Vec<SessionId> {
        let mut expired_sessions = Vec::new();
        let _now = SystemTime::now();

        // Identify expired sessions
        {
            let sessions_read = sessions.read().await;
            for (session_id, metadata) in sessions_read.iter() {
                let should_expire = match metadata.status {
                    SessionStatus::Idle => metadata.idle_time() > config.max_idle_time,
                    SessionStatus::Active | SessionStatus::Connecting => {
                        metadata.age() > config.max_session_lifetime
                    }
                    SessionStatus::Failed => true,
                    _ => false,
                };

                if should_expire {
                    expired_sessions.push(*session_id);
                }
            }
        }

        // Remove expired sessions
        if !expired_sessions.is_empty() {
            let mut sessions_write = sessions.write().await;
            let mut pools_write = connection_pools.write().await;

            for session_id in &expired_sessions {
                if let Some(metadata) = sessions_write.remove(session_id) {
                    warn!(
                        "Expired session {} ({}) after {:?}",
                        session_id,
                        metadata.name,
                        metadata.age()
                    );

                    // Remove from connection pools
                    for pool in pools_write.values_mut() {
                        pool.retain(|&id| id != *session_id);
                    }
                }
            }

            // Update statistics
            let mut stats_lock = stats.lock().await;
            stats_lock.active_sessions = sessions_write.len() as u32;
            stats_lock.idle_sessions = sessions_write
                .values()
                .filter(|m| m.status == SessionStatus::Idle)
                .count() as u32;
        }

        expired_sessions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{GeneralTransportConfig, LocalTransportConfig};
    use std::path::PathBuf;

    #[test]
    fn test_session_id_creation() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1, id2);

        let id_str = id1.as_str();
        let parsed_id: SessionId = id_str.parse().unwrap();
        assert_eq!(id1, parsed_id);
    }

    #[test]
    fn test_session_metadata_creation() {
        let transport_config = TransportConfig {
            transport_type: TransportType::Local,
            local: Some(LocalTransportConfig {
                binary_path: PathBuf::from("test"),
                args: vec![],
                working_dir: None,
            }),
            ssh: None,
            tcp: None,
            wsl: None,
            general: GeneralTransportConfig::default(),
        };

        let metadata = SessionMetadata::new(
            "test-session".to_string(),
            transport_config,
            vec!["test".to_string()],
            Some("Test session".to_string()),
        );

        assert_eq!(metadata.name, "test-session");
        assert_eq!(metadata.status, SessionStatus::Connecting);
        assert!(metadata.has_tags(&["test".to_string()]));
        assert_eq!(metadata.use_count, 0);
    }

    #[tokio::test]
    async fn test_session_manager_creation() {
        let config = SessionManagerConfig::default();
        let manager = SessionManager::new(config);

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_sessions_created, 0);
        assert_eq!(stats.active_sessions, 0);
    }

    #[tokio::test]
    async fn test_session_creation_and_management() {
        let config = SessionManagerConfig::default();
        let manager = SessionManager::new(config);

        let transport_config = TransportConfig {
            transport_type: TransportType::Local,
            local: Some(LocalTransportConfig {
                binary_path: PathBuf::from("test"),
                args: vec![],
                working_dir: None,
            }),
            ssh: None,
            tcp: None,
            wsl: None,
            general: GeneralTransportConfig::default(),
        };

        // Create session
        let session_id = manager
            .create_session(
                "test-session".to_string(),
                transport_config,
                vec!["test".to_string()],
                None,
            )
            .await
            .unwrap();

        // Check session exists
        let metadata = manager.get_session_metadata(session_id).await.unwrap();
        assert_eq!(metadata.name, "test-session");

        // Use session
        manager.use_session(session_id).await.unwrap();
        let metadata = manager.get_session_metadata(session_id).await.unwrap();
        assert_eq!(metadata.use_count, 1);

        // Close session
        manager.close_session(session_id).await.unwrap();
        assert!(manager.get_session_metadata(session_id).await.is_none());
    }

    #[tokio::test]
    async fn test_session_limits() {
        let mut config = SessionManagerConfig::default();
        config.max_concurrent_sessions = 1;
        let manager = SessionManager::new(config);

        let transport_config = TransportConfig {
            transport_type: TransportType::Local,
            local: Some(LocalTransportConfig {
                binary_path: PathBuf::from("test"),
                args: vec![],
                working_dir: None,
            }),
            ssh: None,
            tcp: None,
            wsl: None,
            general: GeneralTransportConfig::default(),
        };

        // Create first session
        let _session1 = manager
            .create_session(
                "session1".to_string(),
                transport_config.clone(),
                vec![],
                None,
            )
            .await
            .unwrap();

        // Try to create second session (should fail)
        let result = manager
            .create_session("session2".to_string(), transport_config, vec![], None)
            .await;

        assert!(result.is_err());
    }
}
