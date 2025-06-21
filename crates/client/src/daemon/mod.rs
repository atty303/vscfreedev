//! Daemon functionality for managing remote connections
//!
//! This module implements the daemon server that runs in the background
//! and manages persistent connections to remote servers.
//! It can be used by both CLI and GUI applications.

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
