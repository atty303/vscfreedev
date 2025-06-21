//! Legacy transport types for backward compatibility
//!
//! This module provides backward compatibility while migrating to the new
//! unified transport system. New code should use the transport types from transport::mod.

// Re-export the new unified transport system
pub use self::mod_new::*;

// Legacy type aliases for backward compatibility
pub use self::mod_new::{
    SshConfig as SshTransportConfig,
    LocalConfig as LocalTransportConfig,
    TcpConfig as TcpTransportConfig,
    WslConfig as WslTransportConfig,
    GeneralConfig as GeneralTransportConfig,
};

// Import the new transport system
#[path = "transport/mod.rs"]
mod mod_new;