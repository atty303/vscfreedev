use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;
use yuha_client::simple_client;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
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
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::Ssh {
            host,
            port,
            username,
            password,
            key_path,
            auto_upload_binary,
        } => {
            info!("Connecting to {}:{} as {}", host, port, username);

            // Connect to the remote host via SSH using new transport
            let client = if *auto_upload_binary {
                simple_client::connect_ssh_with_auto_upload(
                    host,
                    *port,
                    username,
                    password.as_deref(),
                    key_path.as_deref(),
                    *auto_upload_binary,
                )
                .await?
            } else {
                simple_client::connect_ssh(
                    host,
                    *port,
                    username,
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

            // Connect to local process
            let client = simple_client::connect_local_process(binary_path.as_deref()).await?;

            info!("Connected to local process");

            // Test clipboard functionality
            let test_message = "Hello from yuha CLI (local)!";
            info!("Setting clipboard to: {}", test_message);
            client.set_clipboard(test_message.to_string()).await?;

            // Get clipboard content
            let clipboard_content = client.get_clipboard().await?;
            info!("Retrieved clipboard content: {}", clipboard_content);

            // Test browser functionality
            info!("Opening browser");
            client
                .open_browser("https://example.com".to_string())
                .await?;

            info!("Local connection test completed successfully");
        }
    }

    Ok(())
}
