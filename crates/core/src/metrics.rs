//! Metrics and monitoring infrastructure for Yuha
//!
//! This module provides comprehensive metrics collection and reporting capabilities
//! for monitoring the performance and health of Yuha connections and operations.

use crate::error::{Result, YuhaError};
use crate::transport::TransportType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info};

/// Global metrics collector
pub static METRICS: once_cell::sync::Lazy<MetricsCollector> =
    once_cell::sync::Lazy::new(MetricsCollector::new);

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,
    /// Metrics collection interval in seconds
    pub collection_interval: u64,
    /// Maximum number of metric samples to keep in memory
    pub max_samples: usize,
    /// Export metrics to external systems
    pub export_enabled: bool,
    /// Export interval in seconds
    pub export_interval: u64,
    /// Export formats (prometheus, json, etc.)
    pub export_formats: Vec<String>,
    /// Export endpoints
    pub export_endpoints: HashMap<String, String>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval: 30,
            max_samples: 1000,
            export_enabled: false,
            export_interval: 60,
            export_formats: vec!["json".to_string()],
            export_endpoints: HashMap::new(),
        }
    }
}

/// Types of metrics that can be collected
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricType {
    /// Counter that only increases
    Counter,
    /// Gauge that can increase or decrease
    Gauge,
    /// Histogram for timing and distribution data
    Histogram,
}

/// A metric measurement with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSample {
    /// Timestamp when the metric was recorded
    pub timestamp: SystemTime,
    /// The measured value
    pub value: f64,
    /// Optional labels for the metric
    pub labels: HashMap<String, String>,
}

/// Metric definition and its current samples
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    /// Name of the metric
    pub name: String,
    /// Type of metric
    pub metric_type: MetricType,
    /// Help/description text
    pub help: String,
    /// Current samples (limited by max_samples config)
    pub samples: Vec<MetricSample>,
    /// Current value (for counters and gauges)
    pub current_value: f64,
}

impl Metric {
    /// Create a new metric
    pub fn new(name: String, metric_type: MetricType, help: String) -> Self {
        Self {
            name,
            metric_type,
            help,
            samples: Vec::new(),
            current_value: 0.0,
        }
    }

    /// Add a sample to the metric
    pub fn add_sample(&mut self, value: f64, labels: HashMap<String, String>, max_samples: usize) {
        let sample = MetricSample {
            timestamp: SystemTime::now(),
            value,
            labels,
        };

        self.samples.push(sample);

        // Update current value based on metric type
        match self.metric_type {
            MetricType::Counter => self.current_value += value,
            MetricType::Gauge => self.current_value = value,
            MetricType::Histogram => {
                // For histograms, current_value could represent the latest value
                self.current_value = value;
            }
        }

        // Limit the number of samples
        if self.samples.len() > max_samples {
            self.samples.remove(0);
        }
    }

    /// Get the latest sample
    pub fn latest_sample(&self) -> Option<&MetricSample> {
        self.samples.last()
    }

    /// Get samples within a time range
    pub fn samples_in_range(&self, start: SystemTime, end: SystemTime) -> Vec<&MetricSample> {
        self.samples
            .iter()
            .filter(|sample| sample.timestamp >= start && sample.timestamp <= end)
            .collect()
    }
}

/// Performance metrics for operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Number of operations performed
    pub operation_count: u64,
    /// Total time spent on operations
    pub total_duration: Duration,
    /// Average operation duration
    pub avg_duration: Duration,
    /// Minimum operation duration
    pub min_duration: Duration,
    /// Maximum operation duration
    pub max_duration: Duration,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f64,
    /// Error count
    pub error_count: u64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            operation_count: 0,
            total_duration: Duration::ZERO,
            avg_duration: Duration::ZERO,
            min_duration: Duration::MAX,
            max_duration: Duration::ZERO,
            success_rate: 1.0,
            error_count: 0,
        }
    }
}

impl PerformanceMetrics {
    /// Record a successful operation
    pub fn record_success(&mut self, duration: Duration) {
        self.operation_count += 1;
        self.total_duration += duration;
        self.avg_duration = self.total_duration / self.operation_count as u32;

        if duration < self.min_duration {
            self.min_duration = duration;
        }
        if duration > self.max_duration {
            self.max_duration = duration;
        }

        self.update_success_rate();
    }

    /// Record a failed operation
    pub fn record_error(&mut self, duration: Option<Duration>) {
        self.operation_count += 1;
        self.error_count += 1;

        if let Some(dur) = duration {
            self.total_duration += dur;
            self.avg_duration = self.total_duration / self.operation_count as u32;

            if dur < self.min_duration {
                self.min_duration = dur;
            }
            if dur > self.max_duration {
                self.max_duration = dur;
            }
        }

        self.update_success_rate();
    }

    fn update_success_rate(&mut self) {
        if self.operation_count > 0 {
            let success_count = self.operation_count - self.error_count;
            self.success_rate = success_count as f64 / self.operation_count as f64;
        }
    }
}

/// Metrics for different transport types
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TransportMetrics {
    /// Connection metrics
    pub connections: PerformanceMetrics,
    /// Data transfer metrics (bytes)
    pub bytes_sent: u64,
    pub bytes_received: u64,
    /// Message metrics
    pub messages_sent: u64,
    pub messages_received: u64,
    /// Latency metrics
    pub latency: PerformanceMetrics,
    /// Reconnection count
    pub reconnections: u64,
}

/// Overall system metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// Metrics by transport type
    pub transport_metrics: HashMap<TransportType, TransportMetrics>,
    /// Feature usage metrics
    pub feature_usage: HashMap<String, u64>,
    /// Memory usage
    pub memory_usage: u64,
    /// CPU usage percentage (0.0 to 100.0)
    pub cpu_usage: f64,
    /// System uptime
    pub uptime: Duration,
    /// Start time
    pub start_time: SystemTime,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            transport_metrics: HashMap::new(),
            feature_usage: HashMap::new(),
            memory_usage: 0,
            cpu_usage: 0.0,
            uptime: Duration::ZERO,
            start_time: SystemTime::now(),
        }
    }
}

/// Timer for measuring operation durations
pub struct Timer {
    start: Instant,
    name: String,
}

impl Timer {
    /// Start a new timer
    pub fn new(name: String) -> Self {
        debug!("Starting timer: {}", name);
        Self {
            start: Instant::now(),
            name,
        }
    }

    /// Stop the timer and return the elapsed duration
    pub fn stop(self) -> Duration {
        let duration = self.start.elapsed();
        debug!("Timer '{}' completed in {:?}", self.name, duration);
        duration
    }

    /// Stop the timer and record the metric
    pub fn stop_and_record(self, collector: &MetricsCollector) -> Duration {
        let name = self.name.clone();
        let duration = self.stop();

        let mut labels = HashMap::new();
        labels.insert("operation".to_string(), name);

        collector.record_histogram("operation_duration_seconds", duration.as_secs_f64(), labels);
        duration
    }
}

/// Main metrics collector
pub struct MetricsCollector {
    config: Arc<RwLock<MetricsConfig>>,
    metrics: Arc<RwLock<HashMap<String, Metric>>>,
    system_metrics: Arc<Mutex<SystemMetrics>>,
    collection_task_running: Arc<Mutex<bool>>,
    export_task_running: Arc<Mutex<bool>>,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(MetricsConfig::default())),
            metrics: Arc::new(RwLock::new(HashMap::new())),
            system_metrics: Arc::new(Mutex::new(SystemMetrics::default())),
            collection_task_running: Arc::new(Mutex::new(false)),
            export_task_running: Arc::new(Mutex::new(false)),
        }
    }

    /// Initialize with configuration
    pub async fn init(&self, config: MetricsConfig) -> Result<()> {
        *self.config.write().await = config.clone();

        if config.enabled {
            self.start_collection_task().await?;

            if config.export_enabled {
                self.start_export_task().await?;
            }
        }

        info!("Metrics collector initialized");
        Ok(())
    }

    /// Record a counter metric
    pub fn counter(&self, name: &str, value: f64, labels: HashMap<String, String>) {
        tokio::spawn({
            let name = name.to_string();
            let metrics = Arc::clone(&self.metrics);
            let config = Arc::clone(&self.config);
            async move {
                {
                    let config_guard = config.read().await;
                    if !config_guard.enabled {
                        return;
                    }
                    let max_samples = config_guard.max_samples;
                    drop(config_guard);

                    let mut metrics_guard = metrics.write().await;
                    let metric = metrics_guard.entry(name.clone()).or_insert_with(|| {
                        Metric::new(
                            name.clone(),
                            MetricType::Counter,
                            format!("Counter metric: {}", name),
                        )
                    });

                    metric.add_sample(value, labels, max_samples);
                }
            }
        });
    }

    /// Record a gauge metric
    pub fn gauge(&self, name: &str, value: f64, labels: HashMap<String, String>) {
        tokio::spawn({
            let name = name.to_string();
            let metrics = Arc::clone(&self.metrics);
            let config = Arc::clone(&self.config);
            async move {
                {
                    let config_guard = config.read().await;
                    if !config_guard.enabled {
                        return;
                    }
                    let max_samples = config_guard.max_samples;
                    drop(config_guard);

                    let mut metrics_guard = metrics.write().await;
                    let metric = metrics_guard.entry(name.clone()).or_insert_with(|| {
                        Metric::new(
                            name.clone(),
                            MetricType::Gauge,
                            format!("Gauge metric: {}", name),
                        )
                    });

                    metric.add_sample(value, labels, max_samples);
                }
            }
        });
    }

    /// Record a histogram metric
    pub fn record_histogram(&self, name: &str, value: f64, labels: HashMap<String, String>) {
        tokio::spawn({
            let name = name.to_string();
            let metrics = Arc::clone(&self.metrics);
            let config = Arc::clone(&self.config);
            async move {
                {
                    let config_guard = config.read().await;
                    if !config_guard.enabled {
                        return;
                    }
                    let max_samples = config_guard.max_samples;
                    drop(config_guard);

                    let mut metrics_guard = metrics.write().await;
                    let metric = metrics_guard.entry(name.clone()).or_insert_with(|| {
                        Metric::new(
                            name.clone(),
                            MetricType::Histogram,
                            format!("Histogram metric: {}", name),
                        )
                    });

                    metric.add_sample(value, labels, max_samples);
                }
            }
        });
    }

    /// Start a timer for an operation
    pub fn start_timer(&self, name: &str) -> Timer {
        Timer::new(name.to_string())
    }

    /// Record transport connection metrics
    pub async fn record_transport_connection(
        &self,
        transport_type: TransportType,
        duration: Duration,
        success: bool,
    ) {
        let mut system_metrics = self.system_metrics.lock().await;
        let transport_metrics = system_metrics
            .transport_metrics
            .entry(transport_type)
            .or_default();

        if success {
            transport_metrics.connections.record_success(duration);
        } else {
            transport_metrics.connections.record_error(Some(duration));
        }
    }

    /// Record data transfer metrics
    pub async fn record_data_transfer(
        &self,
        transport_type: TransportType,
        bytes_sent: u64,
        bytes_received: u64,
    ) {
        let mut system_metrics = self.system_metrics.lock().await;
        let transport_metrics = system_metrics
            .transport_metrics
            .entry(transport_type)
            .or_default();

        transport_metrics.bytes_sent += bytes_sent;
        transport_metrics.bytes_received += bytes_received;
    }

    /// Record feature usage
    pub async fn record_feature_usage(&self, feature: &str) {
        let mut system_metrics = self.system_metrics.lock().await;
        *system_metrics
            .feature_usage
            .entry(feature.to_string())
            .or_insert(0) += 1;
    }

    /// Get current metrics snapshot
    pub async fn get_metrics_snapshot(&self) -> HashMap<String, Metric> {
        self.metrics.read().await.clone()
    }

    /// Get system metrics
    pub async fn get_system_metrics(&self) -> SystemMetrics {
        let mut system_metrics = self.system_metrics.lock().await;
        system_metrics.uptime = SystemTime::now()
            .duration_since(system_metrics.start_time)
            .unwrap_or(Duration::ZERO);
        system_metrics.clone()
    }

    /// Export metrics in JSON format
    pub async fn export_json(&self) -> Result<String> {
        let metrics = self.get_metrics_snapshot().await;
        let system_metrics = self.get_system_metrics().await;

        let export_data = serde_json::json!({
            "timestamp": SystemTime::now(),
            "metrics": metrics,
            "system": system_metrics
        });

        serde_json::to_string_pretty(&export_data)
            .map_err(|e| YuhaError::internal(format!("Failed to serialize metrics: {}", e)))
    }

    /// Start the metrics collection background task
    async fn start_collection_task(&self) -> Result<()> {
        let mut task_running = self.collection_task_running.lock().await;
        if *task_running {
            return Ok(());
        }
        *task_running = true;
        drop(task_running);

        let config = Arc::clone(&self.config);
        let system_metrics = Arc::clone(&self.system_metrics);
        let task_running = Arc::clone(&self.collection_task_running);

        tokio::spawn(async move {
            info!("Starting metrics collection task");

            loop {
                let interval = {
                    let config_guard = config.read().await;
                    Duration::from_secs(config_guard.collection_interval)
                };

                tokio::time::sleep(interval).await;

                if !*task_running.lock().await {
                    break;
                }

                // Collect system metrics (simplified implementation)
                {
                    let mut metrics = system_metrics.lock().await;
                    // In a real implementation, you would collect actual system metrics here
                    metrics.memory_usage = 0; // Placeholder
                    metrics.cpu_usage = 0.0; // Placeholder
                }
            }

            info!("Metrics collection task stopped");
        });

        Ok(())
    }

    /// Start the metrics export background task
    async fn start_export_task(&self) -> Result<()> {
        let mut task_running = self.export_task_running.lock().await;
        if *task_running {
            return Ok(());
        }
        *task_running = true;
        drop(task_running);

        let config = Arc::clone(&self.config);
        let collector = Arc::new(self.clone());
        let task_running = Arc::clone(&self.export_task_running);

        tokio::spawn(async move {
            info!("Starting metrics export task");

            loop {
                let interval = {
                    let config_guard = config.read().await;
                    Duration::from_secs(config_guard.export_interval)
                };

                tokio::time::sleep(interval).await;

                if !*task_running.lock().await {
                    break;
                }

                // Export metrics
                if let Ok(json_data) = collector.export_json().await {
                    debug!("Exported metrics: {} bytes", json_data.len());
                    // In a real implementation, you would send this to external systems
                }
            }

            info!("Metrics export task stopped");
        });

        Ok(())
    }

    /// Stop metrics collection
    pub async fn stop(&self) {
        *self.collection_task_running.lock().await = false;
        *self.export_task_running.lock().await = false;
        info!("Metrics collector stopped");
    }
}

impl Clone for MetricsCollector {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            metrics: Arc::clone(&self.metrics),
            system_metrics: Arc::clone(&self.system_metrics),
            collection_task_running: Arc::clone(&self.collection_task_running),
            export_task_running: Arc::clone(&self.export_task_running),
        }
    }
}

/// Convenience macros for recording metrics
#[macro_export]
macro_rules! metrics_counter {
    ($name:expr, $value:expr) => {
        $crate::metrics::METRICS.counter($name, $value, std::collections::HashMap::new())
    };
    ($name:expr, $value:expr, $labels:expr) => {
        $crate::metrics::METRICS.counter($name, $value, $labels)
    };
}

#[macro_export]
macro_rules! metrics_gauge {
    ($name:expr, $value:expr) => {
        $crate::metrics::METRICS.gauge($name, $value, std::collections::HashMap::new())
    };
    ($name:expr, $value:expr, $labels:expr) => {
        $crate::metrics::METRICS.gauge($name, $value, $labels)
    };
}

#[macro_export]
macro_rules! metrics_histogram {
    ($name:expr, $value:expr) => {
        $crate::metrics::METRICS.record_histogram($name, $value, std::collections::HashMap::new())
    };
    ($name:expr, $value:expr, $labels:expr) => {
        $crate::metrics::METRICS.record_histogram($name, $value, $labels)
    };
}

#[macro_export]
macro_rules! metrics_timer {
    ($name:expr) => {
        $crate::metrics::METRICS.start_timer($name)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_metric_creation() {
        let metric = Metric::new(
            "test_counter".to_string(),
            MetricType::Counter,
            "Test counter metric".to_string(),
        );

        assert_eq!(metric.name, "test_counter");
        assert_eq!(metric.metric_type, MetricType::Counter);
        assert_eq!(metric.current_value, 0.0);
    }

    #[test]
    fn test_metric_sampling() {
        let mut metric = Metric::new(
            "test_gauge".to_string(),
            MetricType::Gauge,
            "Test gauge metric".to_string(),
        );

        let mut labels = HashMap::new();
        labels.insert("test".to_string(), "value".to_string());

        metric.add_sample(42.0, labels, 100);

        assert_eq!(metric.current_value, 42.0);
        assert_eq!(metric.samples.len(), 1);
        assert_eq!(metric.latest_sample().unwrap().value, 42.0);
    }

    #[test]
    fn test_performance_metrics() {
        let mut perf = PerformanceMetrics::default();

        perf.record_success(Duration::from_millis(100));
        perf.record_success(Duration::from_millis(200));
        perf.record_error(Some(Duration::from_millis(50)));

        assert_eq!(perf.operation_count, 3);
        assert_eq!(perf.error_count, 1);
        assert!((perf.success_rate - 0.6666666666666666).abs() < f64::EPSILON);
    }

    #[test]
    fn test_timer() {
        let timer = Timer::new("test_operation".to_string());
        std::thread::sleep(Duration::from_millis(10));
        let duration = timer.stop();

        assert!(duration >= Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_metrics_collector() {
        let collector = MetricsCollector::new();

        let mut labels = HashMap::new();
        labels.insert("component".to_string(), "test".to_string());

        collector.counter("test_counter", 1.0, labels.clone());
        collector.gauge("test_gauge", 42.0, labels.clone());
        collector.record_histogram("test_histogram", 3.14, labels);

        // Give some time for async operations to complete
        tokio::time::sleep(Duration::from_millis(10)).await;

        let metrics = collector.get_metrics_snapshot().await;
        assert!(metrics.contains_key("test_counter"));
        assert!(metrics.contains_key("test_gauge"));
        assert!(metrics.contains_key("test_histogram"));
    }

    #[tokio::test]
    async fn test_system_metrics() {
        let collector = MetricsCollector::new();

        collector
            .record_transport_connection(TransportType::Local, Duration::from_millis(100), true)
            .await;

        collector
            .record_data_transfer(TransportType::Local, 1024, 2048)
            .await;
        collector.record_feature_usage("clipboard").await;

        let system_metrics = collector.get_system_metrics().await;

        assert!(
            system_metrics
                .transport_metrics
                .contains_key(&TransportType::Local)
        );
        assert_eq!(
            system_metrics.transport_metrics[&TransportType::Local].bytes_sent,
            1024
        );
        assert_eq!(
            system_metrics.transport_metrics[&TransportType::Local].bytes_received,
            2048
        );
        assert_eq!(system_metrics.feature_usage["clipboard"], 1);
    }
}
