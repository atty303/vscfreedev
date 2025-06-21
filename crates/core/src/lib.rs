//! # Yuha Core Library
//!
//! This crate provides the core functionality shared between client and remote components
//! of the Yuha remote development tool. It includes protocol definitions, transport
//! abstractions, configuration management, and utility functions.
//!
//! ## Key Components
//!
//! - **Protocol**: Request/response protocol definitions for client-server communication
//! - **Transport**: Abstraction layer for different connection types (SSH, TCP, local)
//! - **Session Management**: Multi-connection session handling and lifecycle management
//! - **Message Channel**: Binary message framing and JSON serialization
//! - **Configuration**: Centralized configuration management
//! - **Metrics & Logging**: Observability and debugging infrastructure
//!
//! ## Architecture
//!
//! The core library follows a simple request-response pattern with long polling
//! for pseudo-bidirectional communication, prioritizing simplicity and reliability
//! over complex bidirectional messaging.

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
