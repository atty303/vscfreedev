//! Session registry for managing session metadata and lookup

use super::{SessionId, SessionMetadata, SessionStatus};
use crate::error::{Result, SessionError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Configuration for the session registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRegistryConfig {
    /// Maximum number of sessions that can be registered
    pub max_sessions: u32,
    /// Whether to allow duplicate session names
    pub allow_duplicate_names: bool,
    /// Enable session name indexing for faster lookup
    pub enable_name_index: bool,
}

impl Default for SessionRegistryConfig {
    fn default() -> Self {
        Self {
            max_sessions: 1000,
            allow_duplicate_names: true,
            enable_name_index: true,
        }
    }
}

/// Session registry that manages session metadata and provides lookup capabilities
pub struct SessionRegistry {
    config: SessionRegistryConfig,
    /// Map of session ID to session metadata
    sessions: Arc<RwLock<HashMap<SessionId, SessionMetadata>>>,
    /// Optional name index for faster name-based lookups
    name_index: Arc<RwLock<HashMap<String, Vec<SessionId>>>>,
    /// Connection key index for grouping similar connections
    connection_index: Arc<RwLock<HashMap<String, Vec<SessionId>>>>,
}

impl SessionRegistry {
    /// Create a new session registry
    pub fn new(config: SessionRegistryConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            name_index: Arc::new(RwLock::new(HashMap::new())),
            connection_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new session
    pub async fn register(&self, mut metadata: SessionMetadata) -> Result<SessionId> {
        let session_id = metadata.id;

        // Check session limit
        {
            let sessions = self.sessions.read().await;
            if sessions.len() >= self.config.max_sessions as usize {
                return Err(SessionError::MaxSessionsReached {
                    limit: self.config.max_sessions,
                }
                .into());
            }

            // Check for duplicate names if not allowed
            if !self.config.allow_duplicate_names {
                if let Some(existing_ids) = self.name_index.read().await.get(&metadata.name) {
                    if !existing_ids.is_empty() {
                        return Err(SessionError::AlreadyExists {
                            session_id: metadata.name.clone(),
                        }
                        .into());
                    }
                }
            }
        }

        // Set initial status if not set
        if matches!(metadata.status, SessionStatus::Creating) {
            metadata.status = SessionStatus::Connecting;
        }

        let connection_key = metadata.connection_key();

        // Register the session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, metadata.clone());
        }

        // Update name index
        if self.config.enable_name_index {
            let mut name_index = self.name_index.write().await;
            name_index
                .entry(metadata.name.clone())
                .or_default()
                .push(session_id);
        }

        // Update connection index
        {
            let mut connection_index = self.connection_index.write().await;
            connection_index
                .entry(connection_key)
                .or_default()
                .push(session_id);
        }

        info!("Registered session {} ({})", session_id, metadata.name);
        debug!("Session metadata: {:?}", metadata);

        Ok(session_id)
    }

    /// Unregister a session
    pub async fn unregister(&self, session_id: SessionId) -> Result<Option<SessionMetadata>> {
        let metadata = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&session_id)
        };

        if let Some(ref metadata) = metadata {
            // Update name index
            if self.config.enable_name_index {
                let mut name_index = self.name_index.write().await;
                if let Some(ids) = name_index.get_mut(&metadata.name) {
                    ids.retain(|&id| id != session_id);
                    if ids.is_empty() {
                        name_index.remove(&metadata.name);
                    }
                }
            }

            // Update connection index
            let connection_key = metadata.connection_key();
            {
                let mut connection_index = self.connection_index.write().await;
                if let Some(ids) = connection_index.get_mut(&connection_key) {
                    ids.retain(|&id| id != session_id);
                    if ids.is_empty() {
                        connection_index.remove(&connection_key);
                    }
                }
            }

            info!("Unregistered session {} ({})", session_id, metadata.name);
        } else {
            warn!(
                "Attempted to unregister non-existent session {}",
                session_id
            );
        }

        Ok(metadata)
    }

    /// Get session metadata by ID
    pub async fn get(&self, session_id: SessionId) -> Option<SessionMetadata> {
        self.sessions.read().await.get(&session_id).cloned()
    }

    /// Update session metadata
    pub async fn update(&self, session_id: SessionId, metadata: SessionMetadata) -> Result<()> {
        if metadata.id != session_id {
            return Err(SessionError::InvalidState {
                current_state: "metadata ID mismatch".to_string(),
                expected_state: "matching session ID".to_string(),
            }
            .into());
        }

        let mut sessions = self.sessions.write().await;
        if let Some(old_metadata) = sessions.get(&session_id) {
            // Update connection index if connection key changed
            let old_connection_key = old_metadata.connection_key();
            let new_connection_key = metadata.connection_key();

            if old_connection_key != new_connection_key {
                let mut connection_index = self.connection_index.write().await;

                // Remove from old connection group
                if let Some(ids) = connection_index.get_mut(&old_connection_key) {
                    ids.retain(|&id| id != session_id);
                    if ids.is_empty() {
                        connection_index.remove(&old_connection_key);
                    }
                }

                // Add to new connection group
                connection_index
                    .entry(new_connection_key)
                    .or_default()
                    .push(session_id);
            }

            // Update name index if name changed
            if self.config.enable_name_index && old_metadata.name != metadata.name {
                let mut name_index = self.name_index.write().await;

                // Remove from old name group
                if let Some(ids) = name_index.get_mut(&old_metadata.name) {
                    ids.retain(|&id| id != session_id);
                    if ids.is_empty() {
                        name_index.remove(&old_metadata.name);
                    }
                }

                // Add to new name group
                name_index
                    .entry(metadata.name.clone())
                    .or_default()
                    .push(session_id);
            }

            sessions.insert(session_id, metadata);
            debug!("Updated session {} metadata", session_id);
            Ok(())
        } else {
            Err(SessionError::NotFound {
                session_id: session_id.to_string(),
            }
            .into())
        }
    }

    /// Update session status
    pub async fn update_status(&self, session_id: SessionId, status: SessionStatus) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(metadata) = sessions.get_mut(&session_id) {
            let old_status = metadata.status;
            metadata.set_status(status);
            debug!(
                "Session {} status changed: {} -> {}",
                session_id, old_status, status
            );
            Ok(())
        } else {
            Err(SessionError::NotFound {
                session_id: session_id.to_string(),
            }
            .into())
        }
    }

    /// Mark a session as used
    pub async fn mark_used(&self, session_id: SessionId) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(metadata) = sessions.get_mut(&session_id) {
            metadata.mark_used();
            debug!(
                "Session {} marked as used (count: {})",
                session_id, metadata.use_count
            );
            Ok(())
        } else {
            Err(SessionError::NotFound {
                session_id: session_id.to_string(),
            }
            .into())
        }
    }

    /// List all sessions
    pub async fn list_all(&self) -> Vec<SessionMetadata> {
        self.sessions.read().await.values().cloned().collect()
    }

    /// List sessions by status
    pub async fn list_by_status(&self, status: SessionStatus) -> Vec<SessionMetadata> {
        self.sessions
            .read()
            .await
            .values()
            .filter(|metadata| metadata.status == status)
            .cloned()
            .collect()
    }

    /// List sessions by name
    pub async fn list_by_name(&self, name: &str) -> Vec<SessionMetadata> {
        if self.config.enable_name_index {
            // Use index for faster lookup
            let name_index = self.name_index.read().await;
            if let Some(session_ids) = name_index.get(name) {
                let sessions = self.sessions.read().await;
                session_ids
                    .iter()
                    .filter_map(|&id| sessions.get(&id).cloned())
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            // Linear search
            self.sessions
                .read()
                .await
                .values()
                .filter(|metadata| metadata.name == name)
                .cloned()
                .collect()
        }
    }

    /// List sessions by connection key
    pub async fn list_by_connection_key(&self, connection_key: &str) -> Vec<SessionMetadata> {
        let connection_index = self.connection_index.read().await;
        if let Some(session_ids) = connection_index.get(connection_key) {
            let sessions = self.sessions.read().await;
            session_ids
                .iter()
                .filter_map(|&id| sessions.get(&id).cloned())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// List sessions by tags
    pub async fn list_by_tags(&self, tags: &[String]) -> Vec<SessionMetadata> {
        self.sessions
            .read()
            .await
            .values()
            .filter(|metadata| metadata.has_tags(tags))
            .cloned()
            .collect()
    }

    /// Get session count
    pub async fn count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Get session count by status
    pub async fn count_by_status(&self, status: SessionStatus) -> usize {
        self.sessions
            .read()
            .await
            .values()
            .filter(|metadata| metadata.status == status)
            .count()
    }

    /// Check if a session exists
    pub async fn exists(&self, session_id: SessionId) -> bool {
        self.sessions.read().await.contains_key(&session_id)
    }

    /// Clear all sessions
    pub async fn clear(&self) {
        let mut sessions = self.sessions.write().await;
        let mut name_index = self.name_index.write().await;
        let mut connection_index = self.connection_index.write().await;

        let count = sessions.len();
        sessions.clear();
        name_index.clear();
        connection_index.clear();

        info!("Cleared {} sessions from registry", count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{TransportBuilder, TransportType};

    #[tokio::test]
    async fn test_session_registration() {
        let config = SessionRegistryConfig::default();
        let registry = SessionRegistry::new(config);

        let transport_config = TransportBuilder::local().build().unwrap();
        let metadata = SessionMetadata::new(
            "test-session",
            transport_config,
            vec!["test".to_string()],
            Some("Test session".to_string()),
        );

        let session_id = metadata.id;
        let registered_id = registry.register(metadata.clone()).await.unwrap();
        assert_eq!(session_id, registered_id);

        // Check that session was registered
        let retrieved = registry.get(session_id).await.unwrap();
        assert_eq!(retrieved.name, "test-session");
        assert_eq!(retrieved.id, session_id);
    }

    #[tokio::test]
    async fn test_session_lookup_by_name() {
        let config = SessionRegistryConfig::default();
        let registry = SessionRegistry::new(config);

        let transport_config = TransportBuilder::local().build().unwrap();
        let metadata = SessionMetadata::new("test-session", transport_config, vec![], None);

        registry.register(metadata).await.unwrap();

        let sessions = registry.list_by_name("test-session").await;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].name, "test-session");

        let no_sessions = registry.list_by_name("nonexistent").await;
        assert_eq!(no_sessions.len(), 0);
    }

    #[tokio::test]
    async fn test_session_status_updates() {
        let config = SessionRegistryConfig::default();
        let registry = SessionRegistry::new(config);

        let transport_config = TransportBuilder::local().build().unwrap();
        let metadata = SessionMetadata::new("test", transport_config, vec![], None);
        let session_id = registry.register(metadata).await.unwrap();

        // Update status
        registry
            .update_status(session_id, SessionStatus::Active)
            .await
            .unwrap();

        let updated = registry.get(session_id).await.unwrap();
        assert_eq!(updated.status, SessionStatus::Active);
    }

    #[tokio::test]
    async fn test_session_limits() {
        let config = SessionRegistryConfig {
            max_sessions: 1,
            ..Default::default()
        };
        let registry = SessionRegistry::new(config);

        let transport_config = TransportBuilder::local().build().unwrap();

        // First session should succeed
        let metadata1 = SessionMetadata::new("session1", transport_config.clone(), vec![], None);
        registry.register(metadata1).await.unwrap();

        // Second session should fail due to limit
        let metadata2 = SessionMetadata::new("session2", transport_config, vec![], None);
        let result = registry.register(metadata2).await;
        assert!(result.is_err());
    }
}
