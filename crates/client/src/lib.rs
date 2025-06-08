//! Client-side library for vscfreedev

use anyhow::{Context, Result};
use ssh2::{Channel as SshChannel, Session};
use std::io::{Read, Write};
use std::net::{TcpStream};
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
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

/// An adapter that implements AsyncRead and AsyncWrite for an SSH channel
pub struct SshChannelAdapter {
    channel: Arc<Mutex<SshChannel>>,
}

impl SshChannelAdapter {
    /// Create a new SSH channel adapter
    pub fn new(channel: SshChannel) -> Self {
        Self {
            channel: Arc::new(Mutex::new(channel)),
        }
    }
}

impl AsyncRead for SshChannelAdapter {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut channel = match self.channel.lock() {
            Ok(channel) => channel,
            Err(_) => return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to lock SSH channel",
            ))),
        };

        let unfilled = buf.initialize_unfilled();
        match channel.read(unfilled) {
            Ok(n) => {
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // SSH2 doesn't support async I/O, so we need to yield and try again later
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl AsyncWrite for SshChannelAdapter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut channel = match self.channel.lock() {
            Ok(channel) => channel,
            Err(_) => return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to lock SSH channel",
            ))),
        };

        match channel.write(buf) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // SSH2 doesn't support async I/O, so we need to yield and try again later
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<()>> {
        // SSH2 doesn't have an explicit flush method
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut channel = match self.channel.lock() {
            Ok(channel) => channel,
            Err(_) => return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to lock SSH channel",
            ))),
        };

        // Send EOF to indicate we're done writing
        if let Err(e) = channel.send_eof() {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to send EOF: {}", e),
            )));
        }

        Poll::Ready(Ok(()))
    }
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
        // Build the remote executable with a timeout
        let remote_executable_path = time::timeout(
            Duration::from_secs(30),
            build_remote_executable()
        ).await.context("Timeout building remote executable")??;

        // Connect to the SSH server
        // Note: TcpStream::connect is not async, so we can't use timeout directly
        let tcp = TcpStream::connect((host, port))
            .with_context(|| format!("Failed to connect to {}:{}", host, port))?;

        // Set a timeout for the TCP connection
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(30)))?;

        // Create an SSH session
        let mut sess = Session::new().context("Failed to create SSH session")?;
        sess.set_tcp_stream(tcp);

        // Set a timeout for the SSH session
        sess.set_timeout(30000); // 30 seconds in milliseconds

        // Perform SSH handshake
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

        // Upload the remote executable with a timeout
        let remote_path = time::timeout(
            Duration::from_secs(60), // Uploading might take longer
            async {
                upload_executable(&sess, &remote_executable_path).context("Failed to upload executable")
            }
        ).await.context("Timeout uploading executable")??;

        // Make the remote executable executable
        let mut channel = sess.channel_session().context("Failed to create SSH channel")?;
        channel.exec(&format!("chmod +x {}", remote_path))
            .context("Failed to make executable executable")?;

        // Read the output with a timeout
        let mut output = String::new();
        time::timeout(
            Duration::from_secs(10),
            async {
                channel.read_to_string(&mut output)?;
                channel.wait_close()?;
                Ok::<_, anyhow::Error>(())
            }
        ).await.context("Timeout waiting for chmod command to complete")??;

        if channel.exit_status()? != 0 {
            return Err(anyhow::anyhow!("Failed to make executable executable: {}", output));
        }

        // Start the remote executable with standard I/O
        let mut channel = sess.channel_session().context("Failed to create SSH channel")?;
        channel.exec(&remote_path)
            .context("Failed to start remote executable")?;

        // Create an adapter for the SSH channel
        let ssh_adapter = SshChannelAdapter::new(channel);

        // Create a message channel using the SSH channel adapter
        let message_channel = MessageChannel::new_with_stream(ssh_adapter);

        // Create a multiplexer
        let multiplexer = Multiplexer::new(message_channel);

        // Create a channel for communication with a timeout
        let channel = time::timeout(
            Duration::from_secs(30),
            async {
                let mut lock = multiplexer.lock().await;
                lock.create_channel().await
            }
        ).await.context("Timeout creating communication channel")??;

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
