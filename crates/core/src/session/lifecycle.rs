//! Session lifecycle management

use super::{SessionId, SessionMetadata, SessionStatus};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::{Instant, interval};
use tracing::{debug, info};

/// Configuration for session lifecycle management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLifecycleConfig {
    /// Maximum idle time before a session is automatically closed
    pub max_idle_time: Duration,
    /// Maximum session lifetime
    pub max_session_lifetime: Duration,
    /// Cleanup interval
    pub cleanup_interval: Duration,
    /// Enable automatic reconnection for failed sessions
    pub enable_auto_reconnect: bool,
    /// Maximum reconnection attempts
    pub max_reconnect_attempts: u32,
    /// Delay between reconnection attempts
    pub reconnect_delay: Duration,
}

impl Default for SessionLifecycleConfig {
    fn default() -> Self {
        Self {
            max_idle_time: Duration::from_secs(300),         // 5 minutes
            max_session_lifetime: Duration::from_secs(3600), // 1 hour
            cleanup_interval: Duration::from_secs(60),       // 1 minute
            enable_auto_reconnect: true,
            max_reconnect_attempts: 3,
            reconnect_delay: Duration::from_secs(5),
        }
    }
}

/// Session lifecycle manager
pub struct SessionLifecycle {
    config: SessionLifecycleConfig,
    cleanup_running: std::sync::Arc<tokio::sync::Mutex<bool>>,
}

impl SessionLifecycle {
    /// Create a new session lifecycle manager
    pub fn new(config: SessionLifecycleConfig) -> Self {
        Self {
            config,
            cleanup_running: std::sync::Arc::new(tokio::sync::Mutex::new(false)),
        }
    }

    /// Start the cleanup task
    pub async fn start_cleanup_task<F, Fut>(&self, cleanup_callback: F) -> Result<()>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Vec<SessionId>> + Send + 'static,
    {
        let mut cleanup_running = self.cleanup_running.lock().await;
        if *cleanup_running {
            return Ok(());
        }
        *cleanup_running = true;
        drop(cleanup_running);

        let cleanup_running = std::sync::Arc::clone(&self.cleanup_running);
        let config = self.config.clone();

        tokio::spawn(async move {
            info!("Starting session lifecycle cleanup task");
            let mut cleanup_interval = interval(config.cleanup_interval);

            loop {
                cleanup_interval.tick().await;

                if !*cleanup_running.lock().await {
                    break;
                }

                // Perform cleanup
                let expired_sessions = cleanup_callback().await;

                if !expired_sessions.is_empty() {
                    info!(
                        "Lifecycle cleanup removed {} expired sessions",
                        expired_sessions.len()
                    );
                }
            }

            info!("Session lifecycle cleanup task stopped");
        });

        Ok(())
    }

    /// Stop the cleanup task
    pub async fn stop_cleanup_task(&self) {
        *self.cleanup_running.lock().await = false;
    }

    /// Check if a session should be expired based on lifecycle rules
    pub fn should_expire_session(&self, session: &SessionMetadata) -> bool {
        let _now = std::time::SystemTime::now();

        // Check idle time
        if matches!(session.status, SessionStatus::Idle)
            && session.idle_time() > self.config.max_idle_time
        {
            debug!(
                "Session {} expired due to idle time: {:?}",
                session.id,
                session.idle_time()
            );
            return true;
        }

        // Check session lifetime
        if session.age() > self.config.max_session_lifetime {
            debug!(
                "Session {} expired due to lifetime: {:?}",
                session.id,
                session.age()
            );
            return true;
        }

        // Always expire failed sessions
        if matches!(session.status, SessionStatus::Failed) {
            debug!("Session {} expired due to failed status", session.id);
            return true;
        }

        false
    }

    /// Check if a session should be reconnected
    pub fn should_reconnect_session(&self, session: &SessionMetadata) -> bool {
        if !self.config.enable_auto_reconnect {
            return false;
        }

        // Only reconnect failed sessions
        if !matches!(session.status, SessionStatus::Failed) {
            return false;
        }

        // Check if session is not too old
        if session.age() > self.config.max_session_lifetime {
            return false;
        }

        // Check if session was used recently
        if session.idle_time() > self.config.max_idle_time {
            return false;
        }

        // Additional logic could check reconnection attempts here
        true
    }

    /// Get lifecycle statistics
    pub fn get_config(&self) -> &SessionLifecycleConfig {
        &self.config
    }

    /// Update lifecycle configuration
    pub fn update_config(&mut self, config: SessionLifecycleConfig) {
        self.config = config;
        info!("Updated session lifecycle configuration");
    }
}

/// Session reconnection manager
pub struct SessionReconnectionManager {
    reconnection_attempts: std::collections::HashMap<SessionId, u32>,
    last_reconnection_time: std::collections::HashMap<SessionId, Instant>,
}

impl SessionReconnectionManager {
    /// Create a new reconnection manager
    pub fn new() -> Self {
        Self {
            reconnection_attempts: std::collections::HashMap::new(),
            last_reconnection_time: std::collections::HashMap::new(),
        }
    }

    /// Check if a session can be reconnected
    pub fn can_reconnect(&self, session_id: SessionId, config: &SessionLifecycleConfig) -> bool {
        // Check attempt count
        let attempts = self.reconnection_attempts.get(&session_id).unwrap_or(&0);
        if *attempts >= config.max_reconnect_attempts {
            return false;
        }

        // Check delay since last attempt
        if let Some(last_time) = self.last_reconnection_time.get(&session_id) {
            if last_time.elapsed() < config.reconnect_delay {
                return false;
            }
        }

        true
    }

    /// Record a reconnection attempt
    pub fn record_reconnection_attempt(&mut self, session_id: SessionId) {
        *self.reconnection_attempts.entry(session_id).or_insert(0) += 1;
        self.last_reconnection_time
            .insert(session_id, Instant::now());
    }

    /// Reset reconnection tracking for a session (on successful reconnect)
    pub fn reset_reconnection(&mut self, session_id: SessionId) {
        self.reconnection_attempts.remove(&session_id);
        self.last_reconnection_time.remove(&session_id);
    }

    /// Clean up tracking for removed sessions
    pub fn cleanup_session(&mut self, session_id: SessionId) {
        self.reconnection_attempts.remove(&session_id);
        self.last_reconnection_time.remove(&session_id);
    }

    /// Get reconnection statistics
    pub fn get_stats(&self) -> ReconnectionStats {
        ReconnectionStats {
            sessions_with_attempts: self.reconnection_attempts.len(),
            total_attempts: self.reconnection_attempts.values().sum(),
            max_attempts_for_session: self
                .reconnection_attempts
                .values()
                .max()
                .copied()
                .unwrap_or(0),
        }
    }
}

impl Default for SessionReconnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about session reconnections
#[derive(Debug, Clone)]
pub struct ReconnectionStats {
    pub sessions_with_attempts: usize,
    pub total_attempts: u32,
    pub max_attempts_for_session: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportBuilder;

    #[test]
    fn test_session_expiration_rules() {
        let config = SessionLifecycleConfig::default();
        let lifecycle = SessionLifecycle::new(config);

        let transport_config = TransportBuilder::local().build().unwrap();
        let mut session =
            super::SessionMetadata::new("test-session", transport_config, vec![], None);

        // New session should not be expired
        assert!(!lifecycle.should_expire_session(&session));

        // Failed session should be expired
        session.set_status(SessionStatus::Failed);
        assert!(lifecycle.should_expire_session(&session));
    }

    #[test]
    fn test_reconnection_manager() {
        let config = SessionLifecycleConfig::default();
        let mut manager = SessionReconnectionManager::new();
        let session_id = super::SessionId::new();

        // Should be able to reconnect initially
        assert!(manager.can_reconnect(session_id, &config));

        // Record attempts up to the limit
        for _ in 0..config.max_reconnect_attempts {
            manager.record_reconnection_attempt(session_id);
        }

        // Should not be able to reconnect after max attempts
        assert!(!manager.can_reconnect(session_id, &config));

        // Reset should allow reconnection again
        manager.reset_reconnection(session_id);
        assert!(manager.can_reconnect(session_id, &config));
    }
}
