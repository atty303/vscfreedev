pub mod browser;
pub mod clipboard;
pub mod config;
pub mod error;
pub mod logging;
pub mod message_channel;
pub mod metrics;
pub mod protocol;
pub mod session;
pub mod transport;

// Re-export commonly used types
pub use config::YuhaConfig;
pub use error::{BrowserError, ClipboardError, ProtocolError as ChannelError, Result, YuhaError};
pub use logging::{LogFormat, LogLevel, LogOutput, LoggerBuilder, LoggingConfig};
pub use metrics::{METRICS, MetricsCollector, MetricsConfig, Timer};
pub use session::{
    SessionId, SessionManager, SessionManagerConfig, SessionMetadata, SessionStats, SessionStatus,
};
pub use transport::{TransportConfig, TransportFactory, TransportType};

#[cfg(test)]
mod tests {
    mod transport_tests;
}
