use anyhow::Result;
use bytes::Bytes;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use vscfreedev_client::client;

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

        /// Message to send to the remote host
        #[arg(short, long, default_value = "Hello from vscfreedev!")]
        message: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Ssh {
            host,
            port,
            username,
            password,
            key_path,
            message,
        } => {
            println!("Connecting to {}:{} as {}", host, port, username);

            // Connect to the remote host via SSH
            let mut channel = client::connect_ssh(
                host,
                *port,
                username,
                password.as_deref(),
                key_path.as_deref(),
            )
            .await?;

            println!("Connected to remote host");

            // Receive the welcome message
            let welcome = channel.receive().await?;
            println!("Received: {}", String::from_utf8_lossy(&welcome));

            // Send a message
            println!("Sending message: {}", message);
            channel.send(Bytes::from(message.clone())).await?;

            // Receive the response
            let response = channel.receive().await?;
            println!("Received: {}", String::from_utf8_lossy(&response));

            println!("SSH connection test completed successfully");
        }
    }

    Ok(())
}
