pub mod browser;
pub mod clipboard;
pub mod config;
pub mod error;
pub mod message_channel;
pub mod protocol;
pub mod transport;

// Re-export commonly used types
pub use config::YuhaConfig;
pub use error::{BrowserError, ChannelError, ClipboardError, Result, YuhaError};
pub use transport::{TransportConfig, TransportFactory, TransportType};

#[cfg(test)]
mod tests {
    mod transport_tests;
}
