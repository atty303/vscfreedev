use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{debug, info};
use yuha_client::client;
use yuha_core::{YuhaConfig, config::ConnectionProfile};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Connection profile to use
    #[arg(short, long)]
    profile: Option<String>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect to a remote host via SSH
    Ssh {
        /// Remote host to connect to
        #[arg(short = 'H', long)]
        host: String,

        /// SSH port
        #[arg(short, long, default_value_t = 22)]
        port: u16,

        /// Username for SSH authentication
        #[arg(short, long)]
        username: String,

        /// Password for SSH authentication (optional)
        #[arg(short = 'P', long)]
        password: Option<String>,

        /// Path to a private key for SSH authentication (optional)
        #[arg(short, long)]
        key_path: Option<PathBuf>,

        /// Automatically upload the remote binary (default: false, use pre-installed binary)
        #[arg(long)]
        auto_upload_binary: bool,

        /// Don't use daemon, connect directly
        #[arg(long)]
        no_daemon: bool,
    },
    /// Connect to a local process
    Local {
        /// Path to the yuha-remote binary (optional, uses built-in path if not specified)
        #[arg(short, long)]
        binary_path: Option<PathBuf>,

        /// Don't use daemon, connect directly
        #[arg(long)]
        no_daemon: bool,
    },
    /// Daemon management
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,

        /// Socket path (Unix) or pipe name (Windows)
        #[arg(short, long)]
        socket: Option<String>,
    },
    /// Stop the daemon
    Stop,
    /// Check daemon status
    Status,
    /// List active sessions
    Sessions,
    /// Show session details
    Info {
        /// Session ID
        session_id: String,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,
    /// Generate default configuration file
    Init {
        /// Path where to create the config file
        #[arg(short, long, default_value = "yuha.toml")]
        output: PathBuf,
    },
    /// List available profiles
    Profiles,
    /// Add a new connection profile
    AddProfile {
        /// Profile name
        name: String,
        /// SSH host
        #[arg(long)]
        host: Option<String>,
        /// SSH port
        #[arg(long)]
        port: Option<u16>,
        /// SSH username
        #[arg(long)]
        username: Option<String>,
        /// SSH key path
        #[arg(long)]
        key_path: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let mut config = if let Some(config_path) = &cli.config {
        YuhaConfig::load_from_file(config_path)?
    } else {
        YuhaConfig::load_with_fallback()
    };

    // Merge with environment variables
    config.merge_with_env();

    // Validate configuration
    config.validate()?;

    // Initialize logging based on configuration
    init_logging(&config, cli.verbose)?;

    debug!("Configuration loaded and validated successfully");
    info!("Yuha CLI starting");

    match &cli.command {
        Commands::Ssh {
            host,
            port,
            username,
            password,
            key_path,
            auto_upload_binary,
            no_daemon,
        } => {
            // Check if profile is specified and override with profile settings
            let (effective_host, effective_port, effective_username, effective_auto_upload) =
                if let Some(profile_name) = &cli.profile {
                    if let Some(profile) = config.get_profile(profile_name) {
                        if let Some(ssh_config) = &profile.ssh {
                            info!("Using SSH profile: {}", profile_name);
                            (
                                ssh_config.host.clone(),
                                ssh_config.port,
                                ssh_config.username.clone(),
                                ssh_config.auto_upload_binary,
                            )
                        } else {
                            return Err(anyhow::anyhow!(
                                "Profile '{}' does not contain SSH configuration",
                                profile_name
                            ));
                        }
                    } else {
                        return Err(anyhow::anyhow!("Profile '{}' not found", profile_name));
                    }
                } else {
                    (host.clone(), *port, username.clone(), *auto_upload_binary)
                };

            info!(
                "Connecting to {}:{} as {}",
                effective_host, effective_port, effective_username
            );

            if *no_daemon {
                // Direct connection without daemon
                let client = if effective_auto_upload {
                    client::connect_ssh_with_auto_upload(
                        &effective_host,
                        effective_port,
                        &effective_username,
                        password.as_deref(),
                        key_path.as_deref(),
                        effective_auto_upload,
                    )
                    .await?
                } else {
                    client::connect_ssh(
                        &effective_host,
                        effective_port,
                        &effective_username,
                        password.as_deref(),
                        key_path.as_deref(),
                    )
                    .await?
                };

                info!("Connected to remote host (direct)");

                // Test clipboard functionality
                let test_message = "Hello from yuha CLI!";
                info!("Setting clipboard to: {}", test_message);
                client.set_clipboard(test_message.to_string()).await?;

                // Get clipboard content
                let clipboard_content = client.get_clipboard().await?;
                info!("Retrieved clipboard content: {}", clipboard_content);

                info!("SSH connection test completed successfully");
            } else {
                // Use daemon for connection
                handle_ssh_via_daemon(
                    &effective_host,
                    effective_port,
                    &effective_username,
                    password.as_deref(),
                    key_path.as_deref(),
                    effective_auto_upload,
                )
                .await?;
            }
        }
        Commands::Local {
            binary_path,
            no_daemon,
        } => {
            info!("Connecting to local process");

            let effective_binary_path = binary_path
                .as_deref()
                .or(config.client.default_binary_path.as_deref());

            if *no_daemon {
                // Direct connection without daemon
                let client = client::connect_local_process(effective_binary_path).await?;

                info!("Connected to local process (direct)");

                // Test clipboard functionality
                let test_message = "Hello from local yuha CLI!";
                info!("Setting clipboard to: {}", test_message);
                client.set_clipboard(test_message.to_string()).await?;

                // Get clipboard content
                let clipboard_content = client.get_clipboard().await?;
                info!("Retrieved clipboard content: {}", clipboard_content);

                // Test browser functionality
                let test_url = "https://example.com";
                info!("Opening browser to: {}", test_url);
                client.open_browser(test_url.to_string()).await?;

                info!("Local connection test completed successfully");
            } else {
                // Use daemon for connection
                handle_local_via_daemon(effective_binary_path).await?;
            }
        }
        Commands::Daemon { action } => {
            handle_daemon_command(action).await?;
        }
        Commands::Config { action } => {
            handle_config_command(action, &config).await?;
        }
    }

    Ok(())
}

/// Initialize logging based on configuration
fn init_logging(config: &YuhaConfig, verbose: bool) -> Result<()> {
    use tracing_subscriber::{EnvFilter, fmt};

    let level = if verbose {
        "debug"
    } else {
        &config.logging.level.to_string()
    };

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    match config.logging.format {
        yuha_core::LogFormat::Json => {
            fmt().json().with_env_filter(env_filter).init();
        }
        yuha_core::LogFormat::Compact => {
            fmt().compact().with_env_filter(env_filter).init();
        }
        yuha_core::LogFormat::Pretty => {
            fmt().pretty().with_env_filter(env_filter).init();
        }
        yuha_core::LogFormat::Full => {
            fmt().with_env_filter(env_filter).init();
        }
    }

    Ok(())
}

/// Handle configuration subcommands
async fn handle_config_command(action: &ConfigAction, config: &YuhaConfig) -> Result<()> {
    match action {
        ConfigAction::Show => {
            println!("Current Configuration:");
            println!("{}", toml::to_string_pretty(config)?);
        }
        ConfigAction::Init { output } => {
            let default_config = YuhaConfig::default();
            default_config.save_to_file(output)?;
            println!("Default configuration saved to: {}", output.display());
        }
        ConfigAction::Profiles => {
            if config.profiles.is_empty() {
                println!("No profiles configured.");
            } else {
                println!("Available profiles:");
                for (name, profile) in &config.profiles {
                    println!("  {}: {}", name, describe_profile(profile));
                }
            }
        }
        ConfigAction::AddProfile {
            name,
            host,
            port,
            username,
            key_path,
        } => {
            if host.is_some() {
                let ssh_config = yuha_core::config::SshConfig {
                    host: host.clone().unwrap_or_default(),
                    port: port.unwrap_or(22),
                    username: username.clone().unwrap_or_default(),
                    password: None,
                    key_path: key_path.clone(),
                    auto_upload_binary: false,
                };

                let profile = ConnectionProfile {
                    name: name.clone(),
                    ssh: Some(ssh_config),
                    local: None,
                    env_vars: std::collections::HashMap::new(),
                    overrides: std::collections::HashMap::new(),
                };

                println!("Profile '{}' configuration:", name);
                println!("{}", toml::to_string_pretty(&profile)?);
                println!("Add this to your yuha.toml file under [profiles.{}]", name);
            } else {
                println!("SSH host is required for profile creation");
            }
        }
    }
    Ok(())
}

/// Describe a connection profile
fn describe_profile(profile: &ConnectionProfile) -> String {
    if let Some(ssh) = &profile.ssh {
        format!("SSH {}@{}:{}", ssh.username, ssh.host, ssh.port)
    } else if let Some(local) = &profile.local {
        format!("Local {}", local.binary_path.display())
    } else {
        "Unknown".to_string()
    }
}

/// Handle SSH connection via daemon
async fn handle_ssh_via_daemon(
    host: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
    key_path: Option<&std::path::Path>,
    _auto_upload_binary: bool,
) -> Result<()> {
    use yuha_client::daemon_client::DaemonClient;

    // Ensure daemon is running
    ensure_daemon_running().await?;

    // Connect to daemon
    let mut daemon_client = DaemonClient::connect(None).await?;

    // Create transport config for SSH
    let mut builder = yuha_core::transport::TransportBuilder::ssh()
        .host(host)
        .port(port)
        .username(username)
        .timeout(30);

    if let Some(pwd) = password {
        builder = builder.password(pwd);
    }

    if let Some(key) = key_path {
        builder = builder.key_file(key);
    }

    let transport_config = builder.build()?;

    // Create or connect to session
    let session_name = format!("ssh-{}@{}:{}", username, host, port);
    let (session_id, reused) = daemon_client
        .connect_session(session_name.clone(), transport_config)
        .await?;

    if reused {
        info!("Reusing existing session: {}", session_name);
    } else {
        info!("Created new session: {}", session_name);
    }

    // Create session client
    let mut session_client =
        yuha_client::daemon_client::DaemonSessionClient::new(daemon_client, session_id);

    // Test clipboard functionality
    let test_message = "Hello from yuha CLI via daemon!";
    info!("Setting clipboard to: {}", test_message);
    session_client
        .set_clipboard(test_message.to_string())
        .await?;

    // Get clipboard content
    let clipboard_content = session_client.get_clipboard().await?;
    info!("Retrieved clipboard content: {}", clipboard_content);

    info!("SSH connection test completed successfully via daemon");
    Ok(())
}

/// Handle local connection via daemon
async fn handle_local_via_daemon(binary_path: Option<&std::path::Path>) -> Result<()> {
    use yuha_client::daemon_client::DaemonClient;

    // Ensure daemon is running
    ensure_daemon_running().await?;

    // Connect to daemon
    let mut daemon_client = DaemonClient::connect(None).await?;

    // Create transport config for local
    let effective_binary_path = binary_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("yuha-remote"));

    let transport_config = yuha_core::transport::TransportBuilder::local()
        .binary_path(effective_binary_path)
        .args(vec!["--stdio".to_string()])
        .build()?;

    // Create or connect to session
    let session_name = "local".to_string();
    let (session_id, reused) = daemon_client
        .connect_session(session_name.clone(), transport_config)
        .await?;

    if reused {
        info!("Reusing existing session: {}", session_name);
    } else {
        info!("Created new session: {}", session_name);
    }

    // Create session client
    let mut session_client =
        yuha_client::daemon_client::DaemonSessionClient::new(daemon_client, session_id);

    // Test clipboard functionality
    let test_message = "Hello from local yuha CLI via daemon!";
    info!("Setting clipboard to: {}", test_message);
    session_client
        .set_clipboard(test_message.to_string())
        .await?;

    // Get clipboard content
    let clipboard_content = session_client.get_clipboard().await?;
    info!("Retrieved clipboard content: {}", clipboard_content);

    // Test browser functionality
    let test_url = "https://example.com";
    info!("Opening browser to: {}", test_url);
    session_client.open_browser(test_url.to_string()).await?;

    info!("Local connection test completed successfully via daemon");
    Ok(())
}

/// Ensure daemon is running, start it if needed
async fn ensure_daemon_running() -> Result<()> {
    use yuha_client::daemon_client::DaemonClient;

    // Try to connect to existing daemon
    match DaemonClient::connect(None).await {
        Ok(mut client) => {
            if client.ping().await? {
                debug!("Daemon is already running");
                return Ok(());
            }
        }
        Err(_) => {
            // Daemon is not running
        }
    }

    // Start daemon in background
    info!("Starting daemon in background...");
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);

    cmd.arg("daemon").arg("start").arg("--foreground");

    // Daemonize
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null());

    cmd.spawn()?;

    // Wait a bit for daemon to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Verify daemon is running
    for _ in 0..10 {
        match DaemonClient::connect(None).await {
            Ok(mut client) => {
                if client.ping().await? {
                    info!("Daemon started successfully");
                    return Ok(());
                }
            }
            Err(_) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }
        }
    }

    Err(anyhow::anyhow!("Failed to start daemon"))
}

/// Handle daemon subcommands
async fn handle_daemon_command(action: &DaemonAction) -> Result<()> {
    use yuha_client::daemon_client::DaemonClient;

    match action {
        DaemonAction::Start { foreground, socket } => {
            if *foreground {
                // Run in foreground
                yuha_client::daemon::run_daemon(
                    None, // config
                    socket.clone(),
                    true,  // foreground
                    None,  // log_file
                    None,  // pid_file
                    false, // verbose
                )
                .await?;
            } else {
                // Build command to start daemon in background
                let exe = std::env::current_exe()?;
                let mut cmd = std::process::Command::new(exe);

                cmd.arg("daemon").arg("start").arg("--foreground");

                if let Some(socket) = socket {
                    cmd.arg("--socket").arg(socket);
                }

                // Daemonize
                cmd.stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .stdin(std::process::Stdio::null());

                cmd.spawn()?;
                println!("Daemon started");
            }
        }
        DaemonAction::Stop => {
            let mut client = DaemonClient::connect(None).await?;
            client.shutdown().await?;
            println!("Daemon shutdown requested");
        }
        DaemonAction::Status => {
            match DaemonClient::connect(None).await {
                Ok(mut client) => {
                    if client.ping().await? {
                        println!("Daemon is running");

                        // Show session count
                        let sessions = client.list_sessions().await?;
                        println!("Active sessions: {}", sessions.len());
                    } else {
                        println!("Daemon is not responding properly");
                    }
                }
                Err(_) => {
                    println!("Daemon is not running");
                }
            }
        }
        DaemonAction::Sessions => {
            let mut client = DaemonClient::connect(None).await?;
            let sessions = client.list_sessions().await?;

            if sessions.is_empty() {
                println!("No active sessions");
            } else {
                println!("Active sessions:");
                println!(
                    "{:<36} {:<20} {:<15} {:<10}",
                    "ID", "Name", "Status", "Uses"
                );
                println!("{}", "-".repeat(80));

                for session in sessions {
                    println!(
                        "{:<36} {:<20} {:<15} {:<10}",
                        session.id.as_str(),
                        session.name,
                        session.status,
                        session.use_count
                    );
                }
            }
        }
        DaemonAction::Info { session_id } => {
            let mut client = DaemonClient::connect(None).await?;
            let session_id: yuha_core::session::SessionId = session_id.parse()?;
            let info = client.get_session_info(session_id).await?;

            println!("Session Information:");
            println!("  ID: {}", info.id);
            println!("  Name: {}", info.name);
            println!("  Status: {}", info.status);
            println!("  Transport: {:?}", info.transport_config.transport_type);
            println!("  Created: {}", info.created_at);
            println!("  Last Used: {}", info.last_used);
            println!("  Use Count: {}", info.use_count);

            if !info.tags.is_empty() {
                println!("  Tags: {}", info.tags.join(", "));
            }

            if let Some(desc) = info.description {
                println!("  Description: {}", desc);
            }
        }
    }

    Ok(())
}
