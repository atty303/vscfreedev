//! Simple client implementation using the new transport abstraction
//!
//! This module provides wrapper functions that use the new transport abstraction
//! while maintaining backward compatibility with existing code.

use anyhow::Result;
use std::path::Path;

use crate::ClientError;
use crate::simple_client_transport::{YuhaClient, connect_local, connect_ssh_transport};
use crate::transport::{LocalTransport, SshTransport, TransportConfig};

// YuhaClient already imported above for backward compatibility

/// Connect to a remote host via SSH and return a YuhaClient
pub async fn connect_ssh(
    host: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
    key_path: Option<&Path>,
) -> Result<YuhaClient<SshTransport>, ClientError> {
    let transport_config = TransportConfig::default();
    connect_ssh_transport(host, port, username, password, key_path, transport_config).await
}

/// Connect to a remote host via SSH with auto-upload option and return a YuhaClient
pub async fn connect_ssh_with_auto_upload(
    host: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
    key_path: Option<&Path>,
    auto_upload_binary: bool,
) -> Result<YuhaClient<SshTransport>, ClientError> {
    let transport_config = TransportConfig {
        auto_upload_binary,
        ..Default::default()
    };
    connect_ssh_transport(host, port, username, password, key_path, transport_config).await
}

/// Connect to a local process and return a YuhaClient
pub async fn connect_local_process(
    binary_path: Option<&Path>,
) -> Result<YuhaClient<LocalTransport>, ClientError> {
    let binary_path = binary_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from(crate::client::get_remote_binary_path()));

    let transport_config = TransportConfig::default();
    connect_local(binary_path, transport_config).await
}
