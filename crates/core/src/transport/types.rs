//! Transport type definitions and enums

use serde::{Deserialize, Serialize};
use std::fmt;

/// Transport types supported by yuha
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    /// SSH connection to remote host
    Ssh,
    /// Local process execution
    Local,
    /// Direct TCP connection
    Tcp,
    /// Windows Subsystem for Linux
    Wsl,
}

impl fmt::Display for TransportType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportType::Ssh => write!(f, "ssh"),
            TransportType::Local => write!(f, "local"),
            TransportType::Tcp => write!(f, "tcp"),
            TransportType::Wsl => write!(f, "wsl"),
        }
    }
}

impl std::str::FromStr for TransportType {
    type Err = crate::error::TransportError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ssh" => Ok(TransportType::Ssh),
            "local" => Ok(TransportType::Local),
            "tcp" => Ok(TransportType::Tcp),
            "wsl" => Ok(TransportType::Wsl),
            _ => Err(crate::error::TransportError::ConfigurationError {
                reason: format!("Unknown transport type: {}", s),
            }),
        }
    }
}

/// Connection state for transport implementations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,
    /// Connecting in progress
    Connecting,
    /// Connected and ready
    Connected,
    /// Connection failed
    Failed,
    /// Reconnecting after failure
    Reconnecting,
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionState::Disconnected => write!(f, "disconnected"),
            ConnectionState::Connecting => write!(f, "connecting"),
            ConnectionState::Connected => write!(f, "connected"),
            ConnectionState::Failed => write!(f, "failed"),
            ConnectionState::Reconnecting => write!(f, "reconnecting"),
        }
    }
}

/// Transport capabilities that can be queried
#[derive(Debug, Clone)]
pub struct TransportCapabilities {
    /// Supports automatic binary upload
    pub auto_upload: bool,
    /// Supports port forwarding
    pub port_forwarding: bool,
    /// Supports secure connections
    pub secure: bool,
    /// Platform-specific transport
    pub platform_specific: bool,
    /// Supports reconnection
    pub reconnectable: bool,
    /// Supports multiplexing
    pub multiplexing: bool,
}

impl TransportCapabilities {
    /// Get capabilities for a transport type
    pub fn for_transport_type(transport_type: TransportType) -> Self {
        match transport_type {
            TransportType::Ssh => Self {
                auto_upload: true,
                port_forwarding: true,
                secure: true,
                platform_specific: false,
                reconnectable: true,
                multiplexing: true,
            },
            TransportType::Local => Self {
                auto_upload: false,
                port_forwarding: false,
                secure: false,
                platform_specific: false,
                reconnectable: false,
                multiplexing: false,
            },
            TransportType::Tcp => Self {
                auto_upload: false,
                port_forwarding: false,
                secure: false, // Depends on TLS configuration
                platform_specific: false,
                reconnectable: true,
                multiplexing: false,
            },
            TransportType::Wsl => Self {
                auto_upload: false,
                port_forwarding: false,
                secure: false,
                platform_specific: true,
                reconnectable: false,
                multiplexing: false,
            },
        }
    }
}
