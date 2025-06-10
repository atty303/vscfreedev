use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{debug, info};
use yuha_client::simple_client;
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
    },
    /// Connect to a local process
    Local {
        /// Path to the yuha-remote binary (optional, uses built-in path if not specified)
        #[arg(short, long)]
        binary_path: Option<PathBuf>,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
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

            // Connect to the remote host via SSH using new transport
            let client = if effective_auto_upload {
                simple_client::connect_ssh_with_auto_upload(
                    &effective_host,
                    effective_port,
                    &effective_username,
                    password.as_deref(),
                    key_path.as_deref(),
                    effective_auto_upload,
                )
                .await?
            } else {
                simple_client::connect_ssh(
                    &effective_host,
                    effective_port,
                    &effective_username,
                    password.as_deref(),
                    key_path.as_deref(),
                )
                .await?
            };

            info!("Connected to remote host");

            // Test clipboard functionality
            let test_message = "Hello from yuha CLI!";
            info!("Setting clipboard to: {}", test_message);
            client.set_clipboard(test_message.to_string()).await?;

            // Get clipboard content
            let clipboard_content = client.get_clipboard().await?;
            info!("Retrieved clipboard content: {}", clipboard_content);

            info!("SSH connection test completed successfully");
        }
        Commands::Local { binary_path } => {
            info!("Connecting to local process");

            let effective_binary_path = binary_path
                .as_deref()
                .or(config.client.default_binary_path.as_deref());

            let client = simple_client::connect_local_process(effective_binary_path).await?;

            info!("Connected to local process");

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
