//! Transport factory for creating transport instances

use super::{Transport, TransportConfig, TransportType};
use crate::error::{Result, TransportError};
use std::sync::Arc;

/// Factory for creating transport instances
pub struct TransportFactory;

impl TransportFactory {
    /// Create a transport instance from configuration
    pub fn create(config: TransportConfig) -> Result<Box<dyn Transport>> {
        // Validate configuration first
        config.validate()?;

        match config.transport_type {
            TransportType::Ssh => Self::create_ssh_transport(config),
            TransportType::Local => Self::create_local_transport(config),
            TransportType::Tcp => Self::create_tcp_transport(config),
            TransportType::Wsl => Self::create_wsl_transport(config),
        }
    }

    /// Create an SSH transport
    fn create_ssh_transport(config: TransportConfig) -> Result<Box<dyn Transport>> {
        let _ssh_config = config
            .ssh
            .ok_or_else(|| TransportError::ConfigurationError {
                reason: "SSH configuration missing".to_string(),
            })?;

        // For now, return a placeholder until we implement the actual SSH transport
        // In a real implementation, this would create the SSH transport instance
        todo!("SSH transport implementation")
    }

    /// Create a local transport
    fn create_local_transport(config: TransportConfig) -> Result<Box<dyn Transport>> {
        let _local_config = config
            .local
            .ok_or_else(|| TransportError::ConfigurationError {
                reason: "Local configuration missing".to_string(),
            })?;

        // For now, return a placeholder until we implement the actual local transport
        // In a real implementation, this would create the local transport instance
        todo!("Local transport implementation")
    }

    /// Create a TCP transport
    fn create_tcp_transport(config: TransportConfig) -> Result<Box<dyn Transport>> {
        let _tcp_config = config
            .tcp
            .ok_or_else(|| TransportError::ConfigurationError {
                reason: "TCP configuration missing".to_string(),
            })?;

        // For now, return a placeholder until we implement the actual TCP transport
        // In a real implementation, this would create the TCP transport instance
        todo!("TCP transport implementation")
    }

    /// Create a WSL transport
    fn create_wsl_transport(config: TransportConfig) -> Result<Box<dyn Transport>> {
        if !cfg!(windows) {
            return Err(TransportError::NotAvailable {
                transport_type: "WSL".to_string(),
                reason: "WSL is only available on Windows".to_string(),
            }
            .into());
        }

        let _wsl_config = config
            .wsl
            .ok_or_else(|| TransportError::ConfigurationError {
                reason: "WSL configuration missing".to_string(),
            })?;

        // For now, return a placeholder until we implement the actual WSL transport
        // In a real implementation, this would create the WSL transport instance
        todo!("WSL transport implementation")
    }

    /// Auto-detect the best available transport for the current platform
    pub fn auto_detect() -> Result<TransportConfig> {
        // Simple heuristics for transport detection
        if cfg!(windows) && Self::is_wsl_available() {
            Ok(super::TransportBuilder::wsl().build()?)
        } else {
            // Default to local transport
            Ok(super::TransportBuilder::local().build()?)
        }
    }

    /// Check if WSL is available (placeholder implementation)
    fn is_wsl_available() -> bool {
        // This would check for WSL installation and available distributions
        // For now, just return false as a placeholder
        false
    }

    /// Get available transport types for the current platform
    pub fn available_transports() -> Vec<TransportType> {
        let mut transports = vec![TransportType::Local, TransportType::Ssh, TransportType::Tcp];

        if cfg!(windows) {
            transports.push(TransportType::Wsl);
        }

        transports
    }

    /// Check if a transport type is available
    pub fn is_transport_available(transport_type: TransportType) -> bool {
        match transport_type {
            TransportType::Ssh | TransportType::Local | TransportType::Tcp => true,
            TransportType::Wsl => cfg!(windows),
        }
    }

    /// Validate a transport configuration without creating an instance
    pub fn validate_config(config: &TransportConfig) -> Result<()> {
        config.validate()
    }
}

/// Transport registry for managing transport implementations
pub struct TransportRegistry {
    transports: std::collections::HashMap<String, Arc<dyn Transport>>,
}

impl TransportRegistry {
    /// Create a new transport registry
    pub fn new() -> Self {
        Self {
            transports: std::collections::HashMap::new(),
        }
    }

    /// Register a transport instance with a name
    pub fn register<S: Into<String>>(&mut self, name: S, transport: Arc<dyn Transport>) {
        self.transports.insert(name.into(), transport);
    }

    /// Get a registered transport by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Transport>> {
        self.transports.get(name).cloned()
    }

    /// List all registered transport names
    pub fn list_names(&self) -> Vec<String> {
        self.transports.keys().cloned().collect()
    }

    /// Remove a transport from the registry
    pub fn unregister(&mut self, name: &str) -> Option<Arc<dyn Transport>> {
        self.transports.remove(name)
    }

    /// Clear all registered transports
    pub fn clear(&mut self) {
        self.transports.clear();
    }
}

impl Default for TransportRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportBuilder;

    #[test]
    fn test_available_transports() {
        let transports = TransportFactory::available_transports();
        assert!(transports.contains(&TransportType::Local));
        assert!(transports.contains(&TransportType::Ssh));
        assert!(transports.contains(&TransportType::Tcp));

        // WSL should only be available on Windows
        assert_eq!(transports.contains(&TransportType::Wsl), cfg!(windows));
    }

    #[test]
    fn test_transport_availability() {
        assert!(TransportFactory::is_transport_available(
            TransportType::Local
        ));
        assert!(TransportFactory::is_transport_available(TransportType::Ssh));
        assert!(TransportFactory::is_transport_available(TransportType::Tcp));
        assert_eq!(
            TransportFactory::is_transport_available(TransportType::Wsl),
            cfg!(windows)
        );
    }

    #[test]
    fn test_config_validation() {
        // Valid local config
        let config = TransportBuilder::local()
            .binary_path("test")
            .build()
            .unwrap();
        assert!(TransportFactory::validate_config(&config).is_ok());

        // Invalid SSH config (missing host)
        let mut config = TransportBuilder::ssh().build().unwrap();
        if let Some(ref mut ssh) = config.ssh {
            ssh.host.clear();
        }
        assert!(TransportFactory::validate_config(&config).is_err());
    }

    #[test]
    fn test_transport_registry() {
        let mut registry = TransportRegistry::new();
        assert_eq!(registry.list_names().len(), 0);

        // Note: We can't actually create transport instances yet
        // since the implementations are not complete
        // This test would be expanded once we have concrete implementations
    }
}
