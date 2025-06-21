//! Session metrics collection and reporting

use super::{SessionId, SessionMetadata, SessionStatus};
use crate::transport::TransportType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Session metrics data
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetrics {
    /// Total number of sessions created
    pub total_sessions_created: u64,
    /// Total number of successful connections
    pub successful_connections: u64,
    /// Total number of failed connections
    pub failed_connections: u64,
    /// Total number of sessions closed
    pub sessions_closed: u64,
    /// Current active sessions
    pub active_sessions: u32,
    /// Current idle sessions
    pub idle_sessions: u32,
    /// Current failed sessions
    pub failed_sessions: u32,
    /// Average session lifetime
    pub avg_session_lifetime: Duration,
    /// Average session idle time
    pub avg_idle_time: Duration,
    /// Session usage by transport type
    pub transport_usage: HashMap<TransportType, u32>,
    /// Connection success rate (percentage)
    pub connection_success_rate: f64,
    /// Session reuse rate (percentage)
    pub session_reuse_rate: f64,
    /// Peak concurrent sessions
    pub peak_concurrent_sessions: u32,
    /// Total session usage count
    pub total_usage_count: u64,
}

/// Metrics collector for session management
pub struct SessionMetricsCollector {
    metrics: Arc<RwLock<SessionMetrics>>,
    /// Historical session data for calculations
    session_history: Arc<RwLock<HashMap<SessionId, SessionHistoryEntry>>>,
    /// Session lifetime tracking
    lifetime_samples: Arc<RwLock<Vec<Duration>>>,
    /// Idle time tracking
    idle_time_samples: Arc<RwLock<Vec<Duration>>>,
    /// Maximum samples to keep in memory
    max_samples: usize,
}

/// Historical data for a session
#[derive(Debug, Clone)]
struct SessionHistoryEntry {
    created_at: SystemTime,
    closed_at: Option<SystemTime>,
    #[allow(dead_code)]
    transport_type: TransportType,
    use_count: u64,
    total_idle_time: Duration,
    was_reused: bool,
}

impl SessionMetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(SessionMetrics::default())),
            session_history: Arc::new(RwLock::new(HashMap::new())),
            lifetime_samples: Arc::new(RwLock::new(Vec::new())),
            idle_time_samples: Arc::new(RwLock::new(Vec::new())),
            max_samples: 1000, // Keep last 1000 samples for averages
        }
    }

    /// Record a new session creation
    pub async fn record_session_created(&self, session: &SessionMetadata) {
        let mut metrics = self.metrics.write().await;
        let mut session_history = self.session_history.write().await;

        metrics.total_sessions_created += 1;

        // Update transport usage
        *metrics
            .transport_usage
            .entry(session.transport_config.transport_type)
            .or_insert(0) += 1;

        // Add to session history
        session_history.insert(
            session.id,
            SessionHistoryEntry {
                created_at: session.created_at,
                closed_at: None,
                transport_type: session.transport_config.transport_type,
                use_count: 0,
                total_idle_time: Duration::ZERO,
                was_reused: false,
            },
        );

        debug!("Recorded session creation: {}", session.id);
    }

    /// Record a successful connection
    pub async fn record_connection_success(&self, session_id: SessionId) {
        let mut metrics = self.metrics.write().await;
        metrics.successful_connections += 1;
        self.update_connection_success_rate(&mut metrics).await;
        debug!("Recorded successful connection for session: {}", session_id);
    }

    /// Record a failed connection
    pub async fn record_connection_failure(&self, session_id: SessionId) {
        let mut metrics = self.metrics.write().await;
        metrics.failed_connections += 1;
        self.update_connection_success_rate(&mut metrics).await;
        debug!("Recorded failed connection for session: {}", session_id);
    }

    /// Record session status change
    pub async fn record_status_change(
        &self,
        session_id: SessionId,
        old_status: SessionStatus,
        new_status: SessionStatus,
    ) {
        let mut metrics = self.metrics.write().await;

        // Update status counters
        match old_status {
            SessionStatus::Active => {
                metrics.active_sessions = metrics.active_sessions.saturating_sub(1)
            }
            SessionStatus::Idle => metrics.idle_sessions = metrics.idle_sessions.saturating_sub(1),
            SessionStatus::Failed => {
                metrics.failed_sessions = metrics.failed_sessions.saturating_sub(1)
            }
            _ => {}
        }

        match new_status {
            SessionStatus::Active => {
                metrics.active_sessions += 1;
                // Update peak concurrent sessions
                let total_active = metrics.active_sessions + metrics.idle_sessions;
                if total_active > metrics.peak_concurrent_sessions {
                    metrics.peak_concurrent_sessions = total_active;
                }
            }
            SessionStatus::Idle => metrics.idle_sessions += 1,
            SessionStatus::Failed => metrics.failed_sessions += 1,
            _ => {}
        }

        debug!(
            "Recorded status change for session {}: {} -> {}",
            session_id, old_status, new_status
        );
    }

    /// Record session usage
    pub async fn record_session_usage(&self, session_id: SessionId, usage_count: u64) {
        let mut metrics = self.metrics.write().await;
        let mut session_history = self.session_history.write().await;

        metrics.total_usage_count += 1;

        // Update session history
        if let Some(history) = session_history.get_mut(&session_id) {
            history.use_count = usage_count;
            if usage_count > 1 {
                history.was_reused = true;
            }
        }

        debug!(
            "Recorded usage for session {}: count = {}",
            session_id, usage_count
        );
    }

    /// Record session closure
    pub async fn record_session_closed(&self, session: &SessionMetadata) {
        let mut metrics = self.metrics.write().await;
        let mut session_history = self.session_history.write().await;
        let mut lifetime_samples = self.lifetime_samples.write().await;
        let mut idle_time_samples = self.idle_time_samples.write().await;

        metrics.sessions_closed += 1;

        // Update session counts
        match session.status {
            SessionStatus::Active => {
                metrics.active_sessions = metrics.active_sessions.saturating_sub(1)
            }
            SessionStatus::Idle => metrics.idle_sessions = metrics.idle_sessions.saturating_sub(1),
            SessionStatus::Failed => {
                metrics.failed_sessions = metrics.failed_sessions.saturating_sub(1)
            }
            _ => {}
        }

        // Update session history and collect samples
        if let Some(history) = session_history.get_mut(&session.id) {
            let now = SystemTime::now();
            history.closed_at = Some(now);

            // Calculate lifetime
            if let Ok(lifetime) = now.duration_since(history.created_at) {
                Self::add_sample(&mut lifetime_samples, lifetime, self.max_samples);
            }

            // Calculate idle time
            let idle_time = session.idle_time();
            history.total_idle_time = idle_time;
            Self::add_sample(&mut idle_time_samples, idle_time, self.max_samples);
        }

        // Recalculate averages
        self.update_averages(&mut metrics, &lifetime_samples, &idle_time_samples)
            .await;
        self.update_reuse_rate(&mut metrics, &session_history).await;

        debug!("Recorded session closure: {}", session.id);
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> SessionMetrics {
        self.metrics.read().await.clone()
    }

    /// Reset all metrics
    pub async fn reset(&self) {
        let mut metrics = self.metrics.write().await;
        let mut session_history = self.session_history.write().await;
        let mut lifetime_samples = self.lifetime_samples.write().await;
        let mut idle_time_samples = self.idle_time_samples.write().await;

        *metrics = SessionMetrics::default();
        session_history.clear();
        lifetime_samples.clear();
        idle_time_samples.clear();

        info!("Reset all session metrics");
    }

    /// Clean up old session history
    pub async fn cleanup_old_history(&self, max_age: Duration) {
        let mut session_history = self.session_history.write().await;
        let now = SystemTime::now();
        let mut removed_count = 0;

        session_history.retain(|_, history| {
            if let Some(closed_at) = history.closed_at {
                if let Ok(age) = now.duration_since(closed_at) {
                    if age > max_age {
                        removed_count += 1;
                        return false;
                    }
                }
            }
            true
        });

        if removed_count > 0 {
            info!("Cleaned up {} old session history entries", removed_count);
        }
    }

    /// Update connection success rate
    async fn update_connection_success_rate(&self, metrics: &mut SessionMetrics) {
        let total_connections = metrics.successful_connections + metrics.failed_connections;
        if total_connections > 0 {
            metrics.connection_success_rate =
                (metrics.successful_connections as f64 / total_connections as f64) * 100.0;
        }
    }

    /// Update session reuse rate
    async fn update_reuse_rate(
        &self,
        metrics: &mut SessionMetrics,
        history: &HashMap<SessionId, SessionHistoryEntry>,
    ) {
        let total_sessions = history.len() as u64;
        if total_sessions > 0 {
            let reused_sessions = history.values().filter(|h| h.was_reused).count() as u64;
            metrics.session_reuse_rate = (reused_sessions as f64 / total_sessions as f64) * 100.0;
        }
    }

    /// Update average calculations
    async fn update_averages(
        &self,
        metrics: &mut SessionMetrics,
        lifetime_samples: &[Duration],
        idle_time_samples: &[Duration],
    ) {
        if !lifetime_samples.is_empty() {
            let total_lifetime: Duration = lifetime_samples.iter().sum();
            metrics.avg_session_lifetime = total_lifetime / lifetime_samples.len() as u32;
        }

        if !idle_time_samples.is_empty() {
            let total_idle: Duration = idle_time_samples.iter().sum();
            metrics.avg_idle_time = total_idle / idle_time_samples.len() as u32;
        }
    }

    /// Add a sample to a collection, maintaining max size
    fn add_sample(samples: &mut Vec<Duration>, sample: Duration, max_size: usize) {
        samples.push(sample);
        if samples.len() > max_size {
            samples.remove(0); // Remove oldest sample
        }
    }
}

impl Default for SessionMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics snapshot for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: SystemTime,
    pub metrics: SessionMetrics,
    pub period: Duration,
}

impl MetricsSnapshot {
    /// Create a new metrics snapshot
    pub fn new(metrics: SessionMetrics, period: Duration) -> Self {
        Self {
            timestamp: SystemTime::now(),
            metrics,
            period,
        }
    }

    /// Get metrics rate per second for the period
    pub fn get_rates(&self) -> MetricsRates {
        let period_secs = self.period.as_secs_f64();

        MetricsRates {
            sessions_created_per_sec: self.metrics.total_sessions_created as f64 / period_secs,
            connections_per_sec: (self.metrics.successful_connections
                + self.metrics.failed_connections) as f64
                / period_secs,
            usage_per_sec: self.metrics.total_usage_count as f64 / period_secs,
        }
    }
}

/// Metrics rates for performance monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsRates {
    pub sessions_created_per_sec: f64,
    pub connections_per_sec: f64,
    pub usage_per_sec: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportBuilder;

    #[tokio::test]
    async fn test_metrics_collection() {
        let collector = SessionMetricsCollector::new();

        let transport_config = TransportBuilder::local().build().unwrap();
        let session = super::SessionMetadata::new("test-session", transport_config, vec![], None);

        // Record session creation
        collector.record_session_created(&session).await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.total_sessions_created, 1);
        assert_eq!(metrics.transport_usage.get(&TransportType::Local), Some(&1));

        // Record successful connection
        collector.record_connection_success(session.id).await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.successful_connections, 1);
        assert!(metrics.connection_success_rate > 0.0);

        // Record status change
        collector
            .record_status_change(session.id, SessionStatus::Connecting, SessionStatus::Active)
            .await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.active_sessions, 1);
    }

    #[tokio::test]
    async fn test_metrics_reset() {
        let collector = SessionMetricsCollector::new();

        let transport_config = TransportBuilder::local().build().unwrap();
        let session = super::SessionMetadata::new("test-session", transport_config, vec![], None);

        collector.record_session_created(&session).await;
        collector.record_connection_success(session.id).await;

        let metrics_before = collector.get_metrics().await;
        assert!(metrics_before.total_sessions_created > 0);

        collector.reset().await;

        let metrics_after = collector.get_metrics().await;
        assert_eq!(metrics_after.total_sessions_created, 0);
        assert_eq!(metrics_after.successful_connections, 0);
    }
}
