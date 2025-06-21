//! Unified session manager that coordinates all session components

use super::{
    SessionId, SessionManagerConfig, SessionMetadata, SessionStatus,
    lifecycle::SessionLifecycle,
    metrics::{SessionMetrics, SessionMetricsCollector},
    pool::SessionPool,
    registry::SessionRegistry,
};
use crate::error::{Result, SessionError};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Unified session manager that coordinates all session management components
pub struct SessionManager {
    /// Session registry for metadata management
    registry: Arc<SessionRegistry>,
    /// Session pool for connection reuse
    pool: Arc<SessionPool>,
    /// Metrics collector for monitoring
    metrics: Option<Arc<SessionMetricsCollector>>,
    /// Lifecycle manager for cleanup and maintenance
    lifecycle: Arc<SessionLifecycle>,
    /// Configuration
    config: SessionManagerConfig,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(config: SessionManagerConfig) -> Self {
        let registry = Arc::new(SessionRegistry::new(config.registry.clone()));
        let pool = Arc::new(SessionPool::new(config.pool.clone()));
        let metrics = if config.enable_metrics {
            Some(Arc::new(SessionMetricsCollector::new()))
        } else {
            None
        };
        let lifecycle = Arc::new(SessionLifecycle::new(config.lifecycle.clone()));

        Self {
            registry,
            pool,
            metrics,
            lifecycle,
            config,
        }
    }

    /// Create a new session
    pub async fn create_session(
        &self,
        name: String,
        transport_config: crate::transport::TransportConfig,
        tags: Vec<String>,
        description: Option<String>,
    ) -> Result<SessionId> {
        let metadata = SessionMetadata::new(name, transport_config, tags, description);
        let session_id = metadata.id;

        // Register the session
        self.registry.register(metadata.clone()).await?;

        // Record metrics
        if let Some(ref metrics) = self.metrics {
            metrics.record_session_created(&metadata).await;
        }

        info!("Created session {} ({})", session_id, metadata.name);
        Ok(session_id)
    }

    /// Get session metadata
    pub async fn get_session(&self, session_id: SessionId) -> Option<SessionMetadata> {
        self.registry.get(session_id).await
    }

    /// Update session status
    pub async fn update_session_status(
        &self,
        session_id: SessionId,
        status: SessionStatus,
    ) -> Result<()> {
        // Get current status for metrics
        let old_status = if let Some(metadata) = self.registry.get(session_id).await {
            metadata.status
        } else {
            return Err(SessionError::NotFound {
                session_id: session_id.to_string(),
            }
            .into());
        };

        // Update in registry
        self.registry.update_status(session_id, status).await?;

        // Handle pooling based on status change
        match status {
            SessionStatus::Idle => {
                // Try to add to pool when becoming idle
                if let Some(metadata) = self.registry.get(session_id).await {
                    self.pool.add_session(&metadata).await?;
                }
            }
            SessionStatus::Active => {
                // Remove from pool when becoming active
                self.pool.remove_session(session_id).await?;
            }
            SessionStatus::Failed | SessionStatus::Closed => {
                // Remove from pool when failed or closed
                self.pool.remove_session(session_id).await?;
            }
            _ => {}
        }

        // Record metrics
        if let Some(ref metrics) = self.metrics {
            metrics
                .record_status_change(session_id, old_status, status)
                .await;

            // Record connection success/failure
            match status {
                SessionStatus::Active => {
                    if matches!(
                        old_status,
                        SessionStatus::Connecting | SessionStatus::Reconnecting
                    ) {
                        metrics.record_connection_success(session_id).await;
                    }
                }
                SessionStatus::Failed => {
                    if matches!(
                        old_status,
                        SessionStatus::Connecting | SessionStatus::Reconnecting
                    ) {
                        metrics.record_connection_failure(session_id).await;
                    }
                }
                _ => {}
            }
        }

        debug!(
            "Session {} status updated: {:?} -> {:?}",
            session_id, old_status, status
        );
        Ok(())
    }

    /// Mark a session as used
    pub async fn use_session(&self, session_id: SessionId) -> Result<()> {
        // Update in registry
        self.registry.mark_used(session_id).await?;

        // Record metrics
        if let Some(ref metrics) = self.metrics {
            if let Some(metadata) = self.registry.get(session_id).await {
                metrics
                    .record_session_usage(session_id, metadata.use_count)
                    .await;
            }
        }

        // If session is idle, make it active
        if let Some(metadata) = self.registry.get(session_id).await {
            if metadata.status == SessionStatus::Idle {
                self.update_session_status(session_id, SessionStatus::Active)
                    .await?;
            }
        }

        Ok(())
    }

    /// Close a session
    pub async fn close_session(&self, session_id: SessionId) -> Result<()> {
        // Get metadata for metrics before removal
        let _metadata = self.registry.get(session_id).await;

        // Update status to closing first
        self.update_session_status(session_id, SessionStatus::Closing)
            .await?;

        // Remove from pool
        self.pool.remove_session(session_id).await?;

        // Remove from registry
        if let Some(removed_metadata) = self.registry.unregister(session_id).await? {
            // Record metrics
            if let Some(ref metrics) = self.metrics {
                metrics.record_session_closed(&removed_metadata).await;
            }

            info!("Closed session {} ({})", session_id, removed_metadata.name);
        }

        Ok(())
    }

    /// Find a session for reuse
    pub async fn find_reusable_session(&self, connection_key: &str) -> Option<SessionId> {
        self.pool.find_session(connection_key).await
    }

    /// Connect to an existing session or create a new one
    pub async fn connect_or_create_session(
        &self,
        name: String,
        transport_config: crate::transport::TransportConfig,
    ) -> Result<(SessionId, bool)> {
        let connection_key = transport_config.connection_key();

        // Try to find a reusable session
        if let Some(session_id) = self.find_reusable_session(&connection_key).await {
            // Mark as used and activate
            self.use_session(session_id).await?;
            info!(
                "Reusing existing session {} for connection '{}'",
                session_id, connection_key
            );
            return Ok((session_id, true));
        }

        // Create a new session
        let session_id = self
            .create_session(name, transport_config, vec![], None)
            .await?;
        info!(
            "Created new session {} for connection '{}'",
            session_id, connection_key
        );
        Ok((session_id, false))
    }

    /// List all sessions
    pub async fn list_sessions(&self) -> Vec<SessionMetadata> {
        self.registry.list_all().await
    }

    /// List sessions by status
    pub async fn list_sessions_by_status(&self, status: SessionStatus) -> Vec<SessionMetadata> {
        self.registry.list_by_status(status).await
    }

    /// List sessions by tags
    pub async fn list_sessions_by_tags(&self, tags: &[String]) -> Vec<SessionMetadata> {
        self.registry.list_by_tags(tags).await
    }

    /// Get session statistics
    pub async fn get_stats(&self) -> Option<SessionMetrics> {
        if let Some(ref metrics) = self.metrics {
            Some(metrics.get_metrics().await)
        } else {
            None
        }
    }

    /// Start the cleanup task
    pub async fn start_cleanup_task(&self) -> Result<()> {
        let registry = Arc::clone(&self.registry);
        let pool = Arc::clone(&self.pool);
        let metrics = self.metrics.clone();
        let lifecycle = Arc::clone(&self.lifecycle);

        let cleanup_callback = move || {
            let registry = Arc::clone(&registry);
            let pool = Arc::clone(&pool);
            let metrics = metrics.clone();
            let lifecycle = Arc::clone(&lifecycle);

            async move {
                let mut expired_sessions = Vec::new();

                // Get all sessions for lifecycle check
                let all_sessions = registry.list_all().await;

                for session in all_sessions {
                    if lifecycle.should_expire_session(&session) {
                        expired_sessions.push(session.id);

                        // Remove from registry and pool
                        if let Ok(Some(metadata)) = registry.unregister(session.id).await {
                            pool.remove_session(session.id).await.ok();

                            // Record metrics
                            if let Some(ref metrics) = metrics {
                                metrics.record_session_closed(&metadata).await;
                            }

                            warn!(
                                "Expired session {} ({}): {:?} old, last used {:?} ago",
                                session.id,
                                session.name,
                                session.age(),
                                session.idle_time()
                            );
                        }
                    }
                }

                // Cleanup expired pooled sessions
                let max_idle_time = lifecycle.get_config().max_idle_time;
                let expired_pooled = pool.cleanup_expired(max_idle_time).await;
                expired_sessions.extend(expired_pooled);

                // Cleanup old metrics history
                if let Some(ref metrics) = metrics {
                    let max_history_age = std::time::Duration::from_secs(86400); // 24 hours
                    metrics.cleanup_old_history(max_history_age).await;
                }

                expired_sessions
            }
        };

        self.lifecycle.start_cleanup_task(cleanup_callback).await
    }

    /// Stop the cleanup task
    pub async fn stop_cleanup_task(&self) {
        self.lifecycle.stop_cleanup_task().await;
    }

    /// Get session count
    pub async fn session_count(&self) -> usize {
        self.registry.count().await
    }

    /// Get pool statistics
    pub async fn get_pool_stats(&self) -> super::pool::PoolStats {
        self.pool.get_stats().await
    }

    /// Check if a session exists
    pub async fn session_exists(&self, session_id: SessionId) -> bool {
        self.registry.exists(session_id).await
    }

    /// Update session metadata
    pub async fn update_session_metadata(
        &self,
        session_id: SessionId,
        metadata: SessionMetadata,
    ) -> Result<()> {
        self.registry.update(session_id, metadata).await
    }

    /// Get configuration
    pub fn get_config(&self) -> &SessionManagerConfig {
        &self.config
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(SessionManagerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportBuilder;

    #[tokio::test]
    async fn test_session_creation_and_retrieval() {
        let config = SessionManagerConfig::default();
        let manager = SessionManager::new(config);

        let transport_config = TransportBuilder::local().build().unwrap();
        let session_id = manager
            .create_session(
                "test-session".to_string(),
                transport_config,
                vec!["test".to_string()],
                Some("Test session".to_string()),
            )
            .await
            .unwrap();

        // Check that session exists
        assert!(manager.session_exists(session_id).await);

        let metadata = manager.get_session(session_id).await.unwrap();
        assert_eq!(metadata.name, "test-session");
        assert_eq!(metadata.id, session_id);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let config = SessionManagerConfig::default();
        let manager = SessionManager::new(config);

        let transport_config = TransportBuilder::local().build().unwrap();
        let session_id = manager
            .create_session("test-session".to_string(), transport_config, vec![], None)
            .await
            .unwrap();

        // Update status to active
        manager
            .update_session_status(session_id, SessionStatus::Active)
            .await
            .unwrap();

        let metadata = manager.get_session(session_id).await.unwrap();
        assert_eq!(metadata.status, SessionStatus::Active);

        // Use session
        manager.use_session(session_id).await.unwrap();

        let metadata = manager.get_session(session_id).await.unwrap();
        assert_eq!(metadata.use_count, 1);

        // Close session
        manager.close_session(session_id).await.unwrap();

        // Session should no longer exist
        assert!(!manager.session_exists(session_id).await);
    }

    #[tokio::test]
    async fn test_session_reuse() {
        let config = SessionManagerConfig::default();
        let manager = SessionManager::new(config);

        let transport_config = TransportBuilder::local().build().unwrap();

        // Create and connect first session
        let (session_id1, reused1) = manager
            .connect_or_create_session("session1".to_string(), transport_config.clone())
            .await
            .unwrap();
        assert!(!reused1);

        // Make session idle for pooling
        manager
            .update_session_status(session_id1, SessionStatus::Idle)
            .await
            .unwrap();

        // Try to connect again with same config
        let (session_id2, reused2) = manager
            .connect_or_create_session("session2".to_string(), transport_config)
            .await
            .unwrap();

        // Should reuse the existing session
        assert!(reused2);
        assert_eq!(session_id1, session_id2);
    }
}
