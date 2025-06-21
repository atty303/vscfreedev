//! Session pooling for connection reuse and management

use super::{SessionId, SessionMetadata, SessionPriority, SessionStatus};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Configuration for session pooling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPoolConfig {
    /// Enable session pooling
    pub enabled: bool,
    /// Maximum number of sessions per connection key
    pub max_sessions_per_connection: u32,
    /// Maximum total pooled sessions
    pub max_total_sessions: u32,
    /// Whether to prefer reusing existing sessions
    pub prefer_reuse: bool,
    /// Minimum idle time before a session can be reused (in seconds)
    pub min_idle_time: u64,
}

impl Default for SessionPoolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_sessions_per_connection: 5,
            max_total_sessions: 50,
            prefer_reuse: true,
            min_idle_time: 5, // 5 seconds
        }
    }
}

/// Session pool for managing connection reuse
pub struct SessionPool {
    config: SessionPoolConfig,
    /// Map of connection key to list of pooled session IDs
    pools: Arc<RwLock<HashMap<String, Vec<SessionId>>>>,
    /// Map of session ID to pool metadata
    pool_metadata: Arc<RwLock<HashMap<SessionId, PooledSessionMetadata>>>,
}

/// Metadata for a pooled session
#[derive(Debug, Clone)]
struct PooledSessionMetadata {
    connection_key: String,
    pooled_at: std::time::SystemTime,
    priority: SessionPriority,
    use_count: u64,
}

impl SessionPool {
    /// Create a new session pool
    pub fn new(config: SessionPoolConfig) -> Self {
        Self {
            config,
            pools: Arc::new(RwLock::new(HashMap::new())),
            pool_metadata: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a session to the pool
    pub async fn add_session(&self, session: &SessionMetadata) -> Result<bool> {
        if !self.config.enabled {
            return Ok(false);
        }

        // Only pool idle or active sessions
        if !matches!(session.status, SessionStatus::Idle | SessionStatus::Active) {
            return Ok(false);
        }

        let connection_key = session.connection_key();

        // Check pool limits
        {
            let pools = self.pools.read().await;
            let pool_metadata = self.pool_metadata.read().await;

            // Check per-connection limit
            if let Some(session_ids) = pools.get(&connection_key) {
                if session_ids.len() >= self.config.max_sessions_per_connection as usize {
                    debug!(
                        "Pool for connection '{}' is at capacity ({})",
                        connection_key,
                        session_ids.len()
                    );
                    return Ok(false);
                }
            }

            // Check total pool limit
            if pool_metadata.len() >= self.config.max_total_sessions as usize {
                debug!("Total pool is at capacity ({})", pool_metadata.len());
                return Ok(false);
            }
        }

        // Add to pool
        {
            let mut pools = self.pools.write().await;
            let mut pool_metadata = self.pool_metadata.write().await;

            pools
                .entry(connection_key.clone())
                .or_default()
                .push(session.id);

            pool_metadata.insert(
                session.id,
                PooledSessionMetadata {
                    connection_key: connection_key.clone(),
                    pooled_at: std::time::SystemTime::now(),
                    priority: session.priority,
                    use_count: session.use_count,
                },
            );
        }

        debug!(
            "Added session {} to pool for '{}'",
            session.id, connection_key
        );
        Ok(true)
    }

    /// Remove a session from the pool
    pub async fn remove_session(&self, session_id: SessionId) -> Result<bool> {
        let mut pools = self.pools.write().await;
        let mut pool_metadata = self.pool_metadata.write().await;

        if let Some(metadata) = pool_metadata.remove(&session_id) {
            // Remove from connection pool
            if let Some(session_ids) = pools.get_mut(&metadata.connection_key) {
                session_ids.retain(|&id| id != session_id);
                if session_ids.is_empty() {
                    pools.remove(&metadata.connection_key);
                }
            }

            debug!(
                "Removed session {} from pool for '{}'",
                session_id, metadata.connection_key
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Find a suitable session from the pool for reuse
    pub async fn find_session(&self, connection_key: &str) -> Option<SessionId> {
        if !self.config.enabled || !self.config.prefer_reuse {
            return None;
        }

        let pools = self.pools.read().await;
        let pool_metadata = self.pool_metadata.read().await;

        if let Some(session_ids) = pools.get(connection_key) {
            // Find the best session to reuse
            let mut best_session = None;
            let mut best_score = 0;

            let min_idle_duration = std::time::Duration::from_secs(self.config.min_idle_time);

            for &session_id in session_ids {
                if let Some(metadata) = pool_metadata.get(&session_id) {
                    // Check minimum idle time
                    if metadata.pooled_at.elapsed().unwrap_or_default() < min_idle_duration {
                        continue;
                    }

                    // Calculate reuse score (higher is better)
                    let score = self.calculate_reuse_score(metadata);
                    if score > best_score {
                        best_score = score;
                        best_session = Some(session_id);
                    }
                }
            }

            if let Some(session_id) = best_session {
                debug!(
                    "Found pooled session {} for connection '{}'",
                    session_id, connection_key
                );
                return Some(session_id);
            }
        }

        None
    }

    /// Calculate a reuse score for a pooled session
    fn calculate_reuse_score(&self, metadata: &PooledSessionMetadata) -> u32 {
        let mut score = 0;

        // Prefer higher priority sessions
        score += match metadata.priority {
            SessionPriority::Critical => 100,
            SessionPriority::High => 75,
            SessionPriority::Normal => 50,
            SessionPriority::Low => 25,
        };

        // Prefer sessions with lower use count (fresher sessions)
        if metadata.use_count < 10 {
            score += 20;
        } else if metadata.use_count < 50 {
            score += 10;
        }

        // Prefer recently pooled sessions (but respect minimum idle time)
        let pooled_duration = metadata.pooled_at.elapsed().unwrap_or_default();
        if pooled_duration < std::time::Duration::from_secs(60) {
            score += 15;
        } else if pooled_duration < std::time::Duration::from_secs(300) {
            score += 10;
        }

        score
    }

    /// List all sessions in a specific pool
    pub async fn list_pool(&self, connection_key: &str) -> Vec<SessionId> {
        self.pools
            .read()
            .await
            .get(connection_key)
            .cloned()
            .unwrap_or_default()
    }

    /// List all connection keys that have pooled sessions
    pub async fn list_connection_keys(&self) -> Vec<String> {
        self.pools.read().await.keys().cloned().collect()
    }

    /// Get pool statistics
    pub async fn get_stats(&self) -> PoolStats {
        let pools = self.pools.read().await;
        let pool_metadata = self.pool_metadata.read().await;

        let total_pooled = pool_metadata.len();
        let connection_count = pools.len();
        let largest_pool = pools.values().map(|v| v.len()).max().unwrap_or(0);

        PoolStats {
            enabled: self.config.enabled,
            total_pooled_sessions: total_pooled,
            connection_pools: connection_count,
            largest_pool_size: largest_pool,
            max_sessions_per_connection: self.config.max_sessions_per_connection,
            max_total_sessions: self.config.max_total_sessions,
        }
    }

    /// Clean expired sessions from pools
    pub async fn cleanup_expired(&self, max_idle_time: std::time::Duration) -> Vec<SessionId> {
        let mut expired_sessions = Vec::new();
        let now = std::time::SystemTime::now();

        {
            let _pools = self.pools.read().await;
            let pool_metadata = self.pool_metadata.read().await;

            for (&session_id, metadata) in pool_metadata.iter() {
                if let Ok(elapsed) = now.duration_since(metadata.pooled_at) {
                    if elapsed > max_idle_time {
                        expired_sessions.push(session_id);
                    }
                }
            }
        }

        // Remove expired sessions
        for session_id in &expired_sessions {
            self.remove_session(*session_id).await.ok();
        }

        if !expired_sessions.is_empty() {
            info!(
                "Cleaned up {} expired pooled sessions",
                expired_sessions.len()
            );
        }

        expired_sessions
    }

    /// Clear all pools
    pub async fn clear(&self) {
        let mut pools = self.pools.write().await;
        let mut pool_metadata = self.pool_metadata.write().await;

        let count = pool_metadata.len();
        pools.clear();
        pool_metadata.clear();

        info!("Cleared {} sessions from all pools", count);
    }

    /// Check if a session is pooled
    pub async fn is_pooled(&self, session_id: SessionId) -> bool {
        self.pool_metadata.read().await.contains_key(&session_id)
    }

    /// Get pool metadata for a session
    pub async fn get_pool_metadata(&self, session_id: SessionId) -> Option<PooledSessionInfo> {
        self.pool_metadata
            .read()
            .await
            .get(&session_id)
            .map(|metadata| PooledSessionInfo {
                connection_key: metadata.connection_key.clone(),
                pooled_at: metadata.pooled_at,
                priority: metadata.priority,
                use_count: metadata.use_count,
            })
    }
}

/// Statistics about session pools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub enabled: bool,
    pub total_pooled_sessions: usize,
    pub connection_pools: usize,
    pub largest_pool_size: usize,
    pub max_sessions_per_connection: u32,
    pub max_total_sessions: u32,
}

/// Information about a pooled session
#[derive(Debug, Clone)]
pub struct PooledSessionInfo {
    pub connection_key: String,
    pub pooled_at: std::time::SystemTime,
    pub priority: SessionPriority,
    pub use_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportBuilder;

    #[tokio::test]
    async fn test_session_pooling() {
        let config = SessionPoolConfig::default();
        let pool = SessionPool::new(config);

        let transport_config = TransportBuilder::local().build().unwrap();
        let mut metadata =
            super::SessionMetadata::new("test-session", transport_config, vec![], None);
        metadata.status = SessionStatus::Idle;

        // Add session to pool
        let added = pool.add_session(&metadata).await.unwrap();
        assert!(added);

        // Find session in pool
        let connection_key = metadata.connection_key();
        let found = pool.find_session(&connection_key).await;
        assert_eq!(found, Some(metadata.id));

        // Remove session from pool
        let removed = pool.remove_session(metadata.id).await.unwrap();
        assert!(removed);

        // Session should no longer be found
        let not_found = pool.find_session(&connection_key).await;
        assert_eq!(not_found, None);
    }

    #[tokio::test]
    async fn test_pool_limits() {
        let config = SessionPoolConfig {
            max_sessions_per_connection: 1,
            ..Default::default()
        };
        let pool = SessionPool::new(config);

        let transport_config = TransportBuilder::local().build().unwrap();

        // First session should be added
        let mut metadata1 =
            super::SessionMetadata::new("session1", transport_config.clone(), vec![], None);
        metadata1.status = SessionStatus::Idle;

        let added1 = pool.add_session(&metadata1).await.unwrap();
        assert!(added1);

        // Second session with same connection key should be rejected
        let mut metadata2 = super::SessionMetadata::new("session2", transport_config, vec![], None);
        metadata2.status = SessionStatus::Idle;

        let added2 = pool.add_session(&metadata2).await.unwrap();
        assert!(!added2);
    }

    #[tokio::test]
    async fn test_pool_stats() {
        let config = SessionPoolConfig::default();
        let pool = SessionPool::new(config.clone());

        let stats = pool.get_stats().await;
        assert_eq!(stats.enabled, config.enabled);
        assert_eq!(stats.total_pooled_sessions, 0);
        assert_eq!(stats.connection_pools, 0);

        // Add a session and check stats again
        let transport_config = TransportBuilder::local().build().unwrap();
        let mut metadata = super::SessionMetadata::new("test", transport_config, vec![], None);
        metadata.status = SessionStatus::Idle;

        pool.add_session(&metadata).await.unwrap();

        let stats = pool.get_stats().await;
        assert_eq!(stats.total_pooled_sessions, 1);
        assert_eq!(stats.connection_pools, 1);
    }
}
