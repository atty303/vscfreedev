//! Client-side library for vscfreedev

use anyhow::{Context, Result};
use ssh2::Session;
use std::io::{Read, Write};
use std::net::{TcpStream};
use std::path::Path;
use std::time::Duration;
use tokio::net::TcpStream as TokioTcpStream;
use tokio::time;
use vscfreedev_core::message_channel::{ChannelHandle, MessageChannel, Multiplexer};

/// Error types for the client
#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("SSH error: {0}")]
    Ssh(#[from] ssh2::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Remote execution error: {0}")]
    RemoteExecution(String),
}

/// Client implementation
pub mod client {
    use super::*;

    /// Initialize the client
    pub fn init() -> Result<()> {
        Ok(())
    }


    /// Connect to a remote host via SSH and establish a message channel
    pub async fn connect_ssh(
        host: &str,
        port: u16,
        username: &str,
        password: Option<&str>,
        key_path: Option<&Path>,
    ) -> Result<ChannelHandle> {
        // Build the remote executable
        let remote_executable_path = build_remote_executable().await?;

        // Connect to the SSH server
        let tcp = TcpStream::connect((host, port))
            .with_context(|| format!("Failed to connect to {}:{}", host, port))?;

        // Create an SSH session
        let mut sess = Session::new().context("Failed to create SSH session")?;
        sess.set_tcp_stream(tcp);
        sess.handshake().context("SSH handshake failed")?;

        // Authenticate
        if let Some(password) = password {
            sess.userauth_password(username, password)
                .context("Password authentication failed")?;
        } else if let Some(key_path) = key_path {
            sess.userauth_pubkey_file(username, None, key_path, None)
                .context("Key authentication failed")?;
        } else {
            // Try agent authentication
            sess.userauth_agent(username)
                .context("Agent authentication failed")?;
        }

        // Upload the remote executable
        let remote_path = upload_executable(&sess, &remote_executable_path).context("Failed to upload executable")?;

        // Make the remote executable executable
        let mut channel = sess.channel_session().context("Failed to create SSH channel")?;
        channel.exec(&format!("chmod +x {}", remote_path))
            .context("Failed to make executable executable")?;
        let mut output = String::new();
        channel.read_to_string(&mut output)?;
        channel.wait_close()?;
        if channel.exit_status()? != 0 {
            return Err(anyhow::anyhow!("Failed to make executable executable: {}", output));
        }

        // Start the remote executable
        let remote_port = 9999; // Default port
        let mut channel = sess.channel_session().context("Failed to create SSH channel")?;
        channel.exec(&format!("{} --port {}", remote_path, remote_port))
            .context("Failed to start remote executable")?;

        // Wait a bit for the remote executable to start
        time::sleep(Duration::from_secs(2)).await;

        // Connect to the remote executable via TCP
        let remote_addr = format!("127.0.0.1:{}", remote_port);
        let stream = TokioTcpStream::connect(&remote_addr).await
            .with_context(|| format!("Failed to connect to remote executable at {}", remote_addr))?;

        // Create a message channel
        let message_channel = MessageChannel::new(stream);

        // Create a multiplexer
        let multiplexer = Multiplexer::new(message_channel);

        // Create a channel for communication
        let channel = multiplexer.lock().await.create_channel().await?;

        Ok(channel)
    }

    /// Build the remote executable
    async fn build_remote_executable() -> Result<String> {
        // For testing, check if we're running in a test environment
        if std::env::var("RUST_TEST").is_ok() {
            // Return a path to an executable that's already running in the Docker container
            return Ok(String::from("/usr/local/bin/vscfreedev_remote"));
        }

        // Return the path to the built executable
        Ok(String::from("../../target/x86_64-unknown-linux-gnu/debug/vscfreedev-remote"))
    }

    /// Upload the executable to the remote host
    fn upload_executable(session: &Session, local_path: &str) -> Result<String> {
        // Read the local executable
        let mut file = std::fs::File::open(local_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        // Create a temporary file on the remote host
        let remote_path = format!("/tmp/vscfreedev_remote_{}", std::process::id());
        let mut remote_file = session.scp_send(
            Path::new(&remote_path),
            0o755, // Executable permissions
            buffer.len() as u64,
            None,
        )?;

        // Write the executable to the remote file
        remote_file.write_all(&buffer)?;

        // Close the remote file
        remote_file.send_eof()?;
        remote_file.wait_eof()?;
        remote_file.close()?;
        remote_file.wait_close()?;

        Ok(remote_path)
    }
}
