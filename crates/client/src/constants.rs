//! Constants and shared values for the yuha client

use std::path::PathBuf;

/// Get the default socket path for daemon communication
pub fn default_socket_path() -> PathBuf {
    if cfg!(unix) {
        PathBuf::from("/tmp/yuha-daemon.sock")
    } else {
        PathBuf::from(r"\\.\pipe\yuha-daemon")
    }
}

/// Get the default socket path as a string
pub fn default_socket_path_str() -> String {
    default_socket_path().to_string_lossy().to_string()
}
