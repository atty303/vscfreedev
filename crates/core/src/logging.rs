//! Unified logging infrastructure for Yuha
//!
//! This module provides standardized logging configuration and utilities
//! for consistent log output across all Yuha components.

use crate::error::{Result, YuhaError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::Level;
use tracing_subscriber::{
    EnvFilter, Registry,
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Global log level
    pub level: LogLevel,
    /// Per-module log levels
    pub module_levels: HashMap<String, LogLevel>,
    /// Log output format
    pub format: LogFormat,
    /// Output destinations
    pub outputs: Vec<LogOutput>,
    /// Include timestamps in logs
    pub include_timestamps: bool,
    /// Include source location (file:line) in logs
    pub include_location: bool,
    /// Include thread information
    pub include_thread_info: bool,
    /// Include span information for tracing
    pub include_spans: bool,
    /// Enable ANSI color codes
    pub enable_colors: bool,
    /// Maximum log file size in bytes (for file output)
    pub max_file_size: u64,
    /// Number of log files to keep when rotating
    pub max_files: u32,
    /// Enable JSON structured logging
    pub structured: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            module_levels: HashMap::new(),
            format: LogFormat::Compact,
            outputs: vec![LogOutput::Stdout],
            include_timestamps: true,
            include_location: false,
            include_thread_info: false,
            include_spans: true,
            enable_colors: true,
            max_file_size: 10 * 1024 * 1024, // 10MB
            max_files: 5,
            structured: false,
        }
    }
}

/// Log levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => Level::TRACE,
            LogLevel::Debug => Level::DEBUG,
            LogLevel::Info => Level::INFO,
            LogLevel::Warn => Level::WARN,
            LogLevel::Error => Level::ERROR,
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "trace"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = YuhaError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "trace" => Ok(LogLevel::Trace),
            "debug" => Ok(LogLevel::Debug),
            "info" => Ok(LogLevel::Info),
            "warn" => Ok(LogLevel::Warn),
            "error" => Ok(LogLevel::Error),
            _ => Err(YuhaError::config(format!("Invalid log level: {}", s))),
        }
    }
}

/// Log output formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Compact single-line format
    Compact,
    /// Pretty multi-line format for development
    Pretty,
    /// Full format with all available information
    Full,
    /// JSON format for structured logging
    Json,
}

/// Log output destinations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LogOutput {
    /// Standard output
    Stdout,
    /// Standard error
    Stderr,
    /// File output
    File {
        /// Path to the log file
        path: PathBuf,
        /// Whether to append to existing file
        append: bool,
    },
    /// Rotating file output
    RotatingFile {
        /// Directory for log files
        directory: PathBuf,
        /// Base filename (without extension)
        filename: String,
    },
    /// Syslog output (Unix systems)
    #[cfg(unix)]
    Syslog {
        /// Syslog facility
        facility: String,
        /// Syslog identifier
        ident: String,
    },
}

/// Logger builder for configuring the logging system
pub struct LoggerBuilder {
    config: LoggingConfig,
}

impl LoggerBuilder {
    /// Create a new logger builder with default configuration
    pub fn new() -> Self {
        Self {
            config: LoggingConfig::default(),
        }
    }

    /// Create a logger builder from configuration
    pub fn from_config(config: LoggingConfig) -> Self {
        Self { config }
    }

    /// Set the global log level
    pub fn level(mut self, level: LogLevel) -> Self {
        self.config.level = level;
        self
    }

    /// Set log level for a specific module
    pub fn module_level<S: Into<String>>(mut self, module: S, level: LogLevel) -> Self {
        self.config.module_levels.insert(module.into(), level);
        self
    }

    /// Set the log format
    pub fn format(mut self, format: LogFormat) -> Self {
        self.config.format = format;
        self
    }

    /// Add a log output destination
    pub fn output(mut self, output: LogOutput) -> Self {
        self.config.outputs.push(output);
        self
    }

    /// Enable or disable timestamps
    pub fn timestamps(mut self, enable: bool) -> Self {
        self.config.include_timestamps = enable;
        self
    }

    /// Enable or disable source location information
    pub fn location(mut self, enable: bool) -> Self {
        self.config.include_location = enable;
        self
    }

    /// Enable or disable thread information
    pub fn thread_info(mut self, enable: bool) -> Self {
        self.config.include_thread_info = enable;
        self
    }

    /// Enable or disable span information
    pub fn spans(mut self, enable: bool) -> Self {
        self.config.include_spans = enable;
        self
    }

    /// Enable or disable colored output
    pub fn colors(mut self, enable: bool) -> Self {
        self.config.enable_colors = enable;
        self
    }

    /// Enable structured JSON logging
    pub fn structured(mut self, enable: bool) -> Self {
        self.config.structured = enable;
        self
    }

    /// Initialize the global logger
    pub fn init(self) -> Result<()> {
        init_logging(self.config)
    }
}

impl Default for LoggerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize the logging system with the given configuration
pub fn init_logging(config: LoggingConfig) -> Result<()> {
    // Build the environment filter
    let mut filter = EnvFilter::new("");

    // Set global level
    filter = filter.add_directive(format!("yuha={}", config.level).parse().unwrap());

    // Add module-specific levels
    for (module, level) in &config.module_levels {
        filter = filter.add_directive(format!("{}={}", module, level).parse().unwrap());
    }

    // Allow RUST_LOG environment variable to override
    if let Ok(env_filter) = std::env::var("RUST_LOG") {
        filter = EnvFilter::new(env_filter);
    }

    let registry = Registry::default().with(filter);

    // Configure the formatter based on the format and outputs
    match config.format {
        _ if config.structured => {
            let fmt_layer = fmt::layer()
                .json()
                .with_current_span(config.include_spans)
                .with_span_list(config.include_spans)
                .with_target(true)
                .with_thread_ids(config.include_thread_info)
                .with_thread_names(config.include_thread_info)
                .with_file(config.include_location)
                .with_line_number(config.include_location);

            registry.with(fmt_layer).init();
        }
        _ => {
            let fmt_layer = fmt::layer()
                .with_target(true)
                .with_thread_ids(config.include_thread_info)
                .with_thread_names(config.include_thread_info)
                .with_file(config.include_location)
                .with_line_number(config.include_location)
                .with_ansi(config.enable_colors)
                .with_span_events(if config.include_spans {
                    FmtSpan::ENTER | FmtSpan::EXIT
                } else {
                    FmtSpan::NONE
                });

            // Create the appropriate layer based on format
            match config.format {
                LogFormat::Compact => {
                    registry.with(fmt_layer.compact()).init();
                }
                LogFormat::Pretty => {
                    registry.with(fmt_layer.pretty()).init();
                }
                LogFormat::Full | LogFormat::Json => {
                    registry.with(fmt_layer).init();
                }
            }
        }
    }

    tracing::info!("Logging system initialized with level: {}", config.level);
    tracing::debug!("Logging configuration: {:?}", config);

    Ok(())
}

/// Initialize logging with environment-based configuration
pub fn init_from_env() -> Result<()> {
    let mut builder = LoggerBuilder::new();

    // Check for YUHA_LOG_LEVEL
    if let Ok(level_str) = std::env::var("YUHA_LOG_LEVEL") {
        let level: LogLevel = level_str.parse()?;
        builder = builder.level(level);
    }

    // Check for YUHA_LOG_FORMAT
    if let Ok(format_str) = std::env::var("YUHA_LOG_FORMAT") {
        let format = match format_str.to_lowercase().as_str() {
            "compact" => LogFormat::Compact,
            "pretty" => LogFormat::Pretty,
            "full" => LogFormat::Full,
            "json" => LogFormat::Json,
            _ => {
                return Err(YuhaError::config(format!(
                    "Invalid log format: {}",
                    format_str
                )));
            }
        };
        builder = builder.format(format);
    }

    // Check for YUHA_LOG_FILE
    if let Ok(file_path) = std::env::var("YUHA_LOG_FILE") {
        builder = builder.output(LogOutput::File {
            path: PathBuf::from(file_path),
            append: true,
        });
    } else {
        // Default to stdout
        builder = builder.output(LogOutput::Stdout);
    }

    // Check for NO_COLOR environment variable
    if std::env::var("NO_COLOR").is_ok() {
        builder = builder.colors(false);
    }

    builder.init()
}

/// Create a development-friendly logger configuration
pub fn init_dev_logging() -> Result<()> {
    LoggerBuilder::new()
        .level(LogLevel::Debug)
        .format(LogFormat::Pretty)
        .output(LogOutput::Stdout)
        .timestamps(true)
        .location(true)
        .thread_info(false)
        .spans(true)
        .colors(true)
        .init()
}

/// Create a production-friendly logger configuration
pub fn init_prod_logging(log_dir: Option<PathBuf>) -> Result<()> {
    let mut builder = LoggerBuilder::new()
        .level(LogLevel::Info)
        .format(LogFormat::Json)
        .timestamps(true)
        .location(false)
        .thread_info(true)
        .spans(false)
        .colors(false)
        .structured(true);

    if let Some(dir) = log_dir {
        builder = builder.output(LogOutput::RotatingFile {
            directory: dir,
            filename: "yuha".to_string(),
        });
    } else {
        builder = builder.output(LogOutput::Stdout);
    }

    builder.init()
}

/// Logging utilities for common patterns
pub mod utils {
    use std::time::Instant;
    use tracing::{Span, debug, error, info, warn};

    /// Log function entry and exit with timing
    pub fn log_function_timing<F, R>(name: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _span = tracing::info_span!("function", name = name).entered();
        let start = Instant::now();

        debug!("Entering function: {}", name);
        let result = f();
        let elapsed = start.elapsed();

        debug!("Exiting function: {} (took {:?})", name, elapsed);
        result
    }

    /// Log an operation with automatic error handling
    pub fn log_operation<F, R, E>(name: &str, operation: F) -> Result<R, E>
    where
        F: FnOnce() -> Result<R, E>,
        E: std::fmt::Display,
    {
        let _span = tracing::info_span!("operation", name = name).entered();

        info!("Starting operation: {}", name);

        match operation() {
            Ok(result) => {
                info!("Operation completed successfully: {}", name);
                Ok(result)
            }
            Err(error) => {
                error!("Operation failed: {} - Error: {}", name, error);
                Err(error)
            }
        }
    }

    /// Create a span for tracking a specific component or feature
    pub fn create_component_span(component: &str) -> Span {
        tracing::info_span!("component", name = component)
    }

    /// Log performance metrics for an operation
    pub fn log_performance(
        operation: &str,
        duration: std::time::Duration,
        items_processed: Option<u64>,
    ) {
        if let Some(items) = items_processed {
            let rate = items as f64 / duration.as_secs_f64();
            info!(
                "Performance: {} completed in {:?} ({} items, {:.2} items/sec)",
                operation, duration, items, rate
            );
        } else {
            info!("Performance: {} completed in {:?}", operation, duration);
        }
    }

    /// Log resource usage information
    pub fn log_resource_usage(component: &str, memory_mb: Option<f64>, cpu_percent: Option<f64>) {
        match (memory_mb, cpu_percent) {
            (Some(mem), Some(cpu)) => {
                debug!(
                    "Resource usage [{}]: Memory: {:.2}MB, CPU: {:.1}%",
                    component, mem, cpu
                );
            }
            (Some(mem), None) => {
                debug!("Memory usage [{}]: {:.2}MB", component, mem);
            }
            (None, Some(cpu)) => {
                debug!("CPU usage [{}]: {:.1}%", component, cpu);
            }
            (None, None) => {
                debug!("Resource usage check requested for {}", component);
            }
        }
    }

    /// Log security-related events
    pub fn log_security_event(event_type: &str, details: &str, severity: SecuritySeverity) {
        match severity {
            SecuritySeverity::Low => info!("Security [{}]: {}", event_type, details),
            SecuritySeverity::Medium => warn!("Security [{}]: {}", event_type, details),
            SecuritySeverity::High => error!("Security [{}]: {}", event_type, details),
            SecuritySeverity::Critical => error!("CRITICAL Security [{}]: {}", event_type, details),
        }
    }

    /// Security event severity levels
    pub enum SecuritySeverity {
        Low,
        Medium,
        High,
        Critical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_log_level_parsing() {
        assert_eq!("debug".parse::<LogLevel>().unwrap(), LogLevel::Debug);
        assert_eq!("info".parse::<LogLevel>().unwrap(), LogLevel::Info);
        assert_eq!("WARN".parse::<LogLevel>().unwrap(), LogLevel::Warn);
        assert!("invalid".parse::<LogLevel>().is_err());
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Debug.to_string(), "debug");
        assert_eq!(LogLevel::Info.to_string(), "info");
        assert_eq!(LogLevel::Warn.to_string(), "warn");
    }

    #[test]
    fn test_logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, LogLevel::Info);
        assert_eq!(config.format, LogFormat::Compact);
        assert_eq!(config.outputs, vec![LogOutput::Stdout]);
        assert!(config.include_timestamps);
        assert!(config.enable_colors);
    }

    #[test]
    fn test_logger_builder() {
        let builder = LoggerBuilder::new()
            .level(LogLevel::Debug)
            .format(LogFormat::Pretty)
            .timestamps(false)
            .colors(false);

        assert_eq!(builder.config.level, LogLevel::Debug);
        assert_eq!(builder.config.format, LogFormat::Pretty);
        assert!(!builder.config.include_timestamps);
        assert!(!builder.config.enable_colors);
    }

    #[test]
    fn test_log_output_serialization() {
        let outputs = vec![
            LogOutput::Stdout,
            LogOutput::Stderr,
            LogOutput::File {
                path: PathBuf::from("/tmp/test.log"),
                append: true,
            },
        ];

        for output in outputs {
            let json = serde_json::to_string(&output).unwrap();
            let deserialized: LogOutput = serde_json::from_str(&json).unwrap();
            assert_eq!(output, deserialized);
        }
    }

    #[test]
    fn test_performance_logging() {
        use std::time::Duration;
        use utils::*;

        // These tests just verify that the functions don't panic
        log_performance("test_operation", Duration::from_millis(100), Some(50));
        log_performance("test_operation", Duration::from_millis(100), None);
        log_resource_usage("test_component", Some(64.5), Some(15.2));
        log_security_event("test_event", "test details", SecuritySeverity::Medium);
    }
}
