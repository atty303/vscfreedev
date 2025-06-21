//! # Daemon Management
//!
//! This module implements the daemon server functionality that runs in the background
//! to manage persistent connections to remote servers. The daemon provides a centralized
//! service that can be shared between multiple client applications (CLI and GUI).
//!
//! ## Key Features
//!
//! - **Persistent Connections**: Maintains long-lived connections to remote servers
//! - **Session Management**: Handles multiple concurrent client sessions
//! - **Resource Sharing**: Allows multiple clients to share the same remote connections
//! - **Background Operation**: Runs as a system service or user daemon
//!
//! ## Architecture
//!
//! ```text
//! CLI Client  →│
//!              │ IPC  │ Daemon  │ SSH/TCP  │ Remote
//! GUI Client  →│     │ Server  │ →       │ Server
//! ```
//!
//! ## Usage
//!
//! The daemon can be started in foreground or background mode:
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use yuha_client::daemon::run_daemon;
//!
//! // Start daemon in foreground for development
//! run_daemon(None, None, true, None, None, false).await?;
//!
//! // Start daemon in background for production
//! run_daemon(
//!     Some("config.toml".into()),
//!     Some("/tmp/yuha.sock".to_string()),
//!     false,
//!     Some("/var/log/yuha.log".into()),
//!     Some("/var/run/yuha.pid".into()),
//!     false
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Configuration
//!
//! The daemon supports configuration via TOML files and command-line arguments.
//! See `DaemonConfig` for available options.

pub mod handler;
pub mod server;

use crate::daemon_protocol::DaemonConfig;
use anyhow::Result;
use std::path::PathBuf;
use tracing::{error, info};

/// Run the daemon with the given configuration
pub async fn run_daemon(
    config: Option<PathBuf>,
    socket: Option<String>,
    foreground: bool,
    log_file: Option<PathBuf>,
    pid_file: Option<PathBuf>,
    verbose: bool,
) -> Result<()> {
    // Initialize logging (only if not already initialized by parent)
    if !foreground {
        init_daemon_logging(log_file.as_ref(), foreground, verbose)?;
    }

    info!("Yuha daemon starting");

    // Load configuration
    let mut daemon_config = if let Some(config_path) = config {
        load_daemon_config(&config_path)?
    } else {
        DaemonConfig::default()
    };

    // Override with command line arguments
    if let Some(socket) = socket {
        daemon_config.socket_path = socket;
    }
    if let Some(log_file) = log_file {
        daemon_config.log_file = Some(log_file.to_string_lossy().to_string());
    }
    if let Some(pid_file) = pid_file {
        daemon_config.pid_file = Some(pid_file.to_string_lossy().to_string());
    }

    // Write PID file if specified
    if let Some(pid_file) = &daemon_config.pid_file {
        write_pid_file(pid_file)?;
    }

    // Start the daemon server
    let server = server::DaemonServer::new(daemon_config);

    // Run the server
    match server.run().await {
        Ok(()) => {
            info!("Yuha daemon stopped");
            Ok(())
        }
        Err(e) => {
            error!("Daemon error: {}", e);
            Err(e)
        }
    }
}

fn init_daemon_logging(log_file: Option<&PathBuf>, foreground: bool, verbose: bool) -> Result<()> {
    use tracing_subscriber::{EnvFilter, fmt};

    let level = if verbose { "debug" } else { "info" };
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    if let Some(log_file) = log_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;

        fmt()
            .with_env_filter(env_filter)
            .with_writer(file)
            .json()
            .init();
    } else if !foreground {
        // In daemon mode, log to stderr which can be redirected
        fmt().with_env_filter(env_filter).json().init();
    }
    // If foreground, assume logging is already initialized by parent

    Ok(())
}

fn load_daemon_config(path: &PathBuf) -> Result<DaemonConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: DaemonConfig = toml::from_str(&content)?;
    Ok(config)
}

fn write_pid_file(path: &str) -> Result<()> {
    let pid = std::process::id();
    std::fs::write(path, pid.to_string())?;
    Ok(())
}
