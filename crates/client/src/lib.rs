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
            Err(e) => {
                println!("ERROR: Failed to lock SSH channel for read: {:?}", e);
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to lock SSH channel",
                )));
            }
        };

        let unfilled = buf.initialize_unfilled();
        match channel.read(unfilled) {
            Ok(0) => {
                // Reading 0 bytes indicates EOF, return EOF error
                println!("SshChannelAdapter::poll_read got 0 bytes (EOF)");
                Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "SSH channel EOF",
                )))
            }
            Ok(n) => {
                println!("SshChannelAdapter::poll_read read {} bytes", n);
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // SSH2 doesn't support async I/O, so we need to yield and try again later
                // Use a small delay to avoid busy waiting
                let waker = cx.waker().clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    waker.wake();
                });
                Poll::Pending
            }
            Err(e) => {
                println!("ERROR: SshChannelAdapter::poll_read error: {:?}", e);
                Poll::Ready(Err(e))
            }
        }
    }
}

impl AsyncWrite for SshChannelAdapter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        println!("SshChannelAdapter::poll_write called with {} bytes", buf.len());

        let mut channel = match self.channel.lock() {
            Ok(channel) => channel,
            Err(e) => {
                println!("ERROR: Failed to lock SSH channel for write: {:?}", e);
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to lock SSH channel",
                )));
            }
        };

        match channel.write(buf) {
            Ok(n) => {
                println!("SshChannelAdapter::poll_write wrote {} bytes", n);
                if n > 0 {
                    // Log the actual data written for debugging
                    let data_str = String::from_utf8_lossy(&buf[..n]);
                    println!("SshChannelAdapter::poll_write data: {}", data_str);
                }

                // Try to flush the channel after writing
                if let Err(e) = channel.flush() {
                    println!("ERROR: SshChannelAdapter::poll_write flush error: {:?}", e);
                    return Poll::Ready(Err(e));
                }

                Poll::Ready(Ok(n))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // SSH2 doesn't support async I/O, so we need to yield and try again later
                // Use a small delay to avoid busy waiting
                let waker = cx.waker().clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    waker.wake();
                });
                Poll::Pending
            }
            Err(e) => {
                println!("ERROR: SshChannelAdapter::poll_write error: {:?}", e);
                Poll::Ready(Err(e))
            }
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<()>> {
        println!("SshChannelAdapter::poll_flush called");

        // SSH2 doesn't have an explicit flush method
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<()>> {
        println!("SshChannelAdapter::poll_shutdown called - NOT sending EOF to keep channel open");

        // Don't send EOF here as it closes stdin and breaks bidirectional communication
        // SSH channels should remain open for continuous communication
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
    /// 
    /// Returns a tuple containing:
    /// - The channel handle for communication
    /// - A boolean indicating whether the welcome message has already been received
    pub async fn connect_ssh(
        host: &str,
        port: u16,
        username: &str,
        password: Option<&str>,
        key_path: Option<&Path>,
    ) -> Result<(ChannelHandle, bool)> {
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

        // First run a simple command to check if SSH is working correctly
        let mut test_channel = sess.channel_session().context("Failed to create SSH channel for test")?;
        println!("Running test command...");
        test_channel.exec("echo 'SSH connection test' && ls -la /usr/local/bin/vscfreedev_remote")
            .context("Failed to run test command")?;

        // Read the output with a timeout
        let mut test_output = String::new();
        time::timeout(
            Duration::from_secs(10),
            async {
                test_channel.read_to_string(&mut test_output)?;
                test_channel.wait_close()?;
                Ok::<_, anyhow::Error>(())
            }
        ).await.context("Timeout waiting for test command to complete")??;

        println!("Test command output: {}", test_output);

        if test_channel.exit_status()? != 0 {
            return Err(anyhow::anyhow!("Test command failed: {}", test_output));
        }

        // First try to run the remote executable with --help to see if it works
        let mut help_channel = sess.channel_session().context("Failed to create SSH channel for help")?;
        println!("Running remote executable with --help...");
        help_channel.exec(&format!("{} --help > /tmp/remote_help.txt 2>&1", remote_path))
            .context("Failed to run remote executable with --help")?;

        // Read the output with a timeout
        let mut help_output = String::new();
        time::timeout(
            Duration::from_secs(10),
            async {
                help_channel.read_to_string(&mut help_output)?;
                help_channel.wait_close()?;
                Ok::<_, anyhow::Error>(())
            }
        ).await.context("Timeout waiting for help command to complete")??;

        println!("Help command exit status: {}", help_channel.exit_status()?);

        // Now run a command to check if the remote executable exists and is executable
        let mut check_channel = sess.channel_session().context("Failed to create SSH channel for check")?;
        println!("Checking remote executable...");
        check_channel.exec(&format!("ls -la {} && file {}", remote_path, remote_path))
            .context("Failed to check remote executable")?;

        // Read the output with a timeout
        let mut check_output = String::new();
        time::timeout(
            Duration::from_secs(10),
            async {
                check_channel.read_to_string(&mut check_output)?;
                check_channel.wait_close()?;
                Ok::<_, anyhow::Error>(())
            }
        ).await.context("Timeout waiting for check command to complete")??;

        println!("Check command output: {}", check_output);

        // Start the remote executable with standard I/O using shell session
        let mut channel = sess.channel_session().context("Failed to create SSH channel")?;
        println!("Starting remote executable via shell: {}", remote_path);

        // Start a shell session to keep stdin open
        channel.shell().context("Failed to start shell")?;
        
        // Send the command to start the remote executable
        let start_cmd = format!("exec {} -s 2>/tmp/remote_stderr.log\n", remote_path);
        channel.write_all(start_cmd.as_bytes()).context("Failed to send start command")?;
        println!("Remote executable started");

        // Read any initial data from the channel
        let mut initial_data = Vec::new();
        let mut buf = [0u8; 1024];

        // Try to read with a timeout to get the welcome message
        let read_result = time::timeout(
            Duration::from_secs(5),
            async {
                for _ in 0..10 {  // Limit iterations to avoid infinite loop
                    match channel.read(&mut buf) {
                        Ok(0) => {
                            println!("SSH channel EOF after starting remote executable");
                            return Err(anyhow::anyhow!("SSH channel EOF immediately"));
                        },
                        Ok(n) => {
                            println!("Read {} bytes from SSH channel", n);
                            initial_data.extend_from_slice(&buf[..n]);

                            // Check if we have a complete VSC message
                            if let Ok(data_str) = std::str::from_utf8(&initial_data) {
                                if data_str.contains("VSC:") && data_str.contains('\n') {
                                    println!("Found complete VSC message in initial data");
                                    break;
                                }
                            }

                            tokio::time::sleep(Duration::from_millis(200)).await;
                        },
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        },
                        Err(e) => {
                            return Err(anyhow::anyhow!("SSH channel error: {}", e));
                        }
                    }
                }
                Ok(())
            }
        ).await;

        if read_result.is_err() {
            println!("Timed out reading initial data, but continuing...");
        }

        println!("Total initial data received: {} bytes", initial_data.len());

        // Create an adapter for the SSH channel
        let ssh_adapter = SshChannelAdapter::new(channel);

        // Create a message channel using text-safe mode for SSH
        let message_channel = MessageChannel::new_with_text_safe_mode(ssh_adapter);

        // Create a multiplexer
        let multiplexer = Multiplexer::new(message_channel);

        // Create a channel for communication with a timeout
        let mut channel = time::timeout(
            Duration::from_secs(30),
            async {
                let mut lock = multiplexer.lock().await;
                lock.create_channel().await
            }
        ).await.context("Timeout creating communication channel")??;

        // Check if we already found a welcome message in the initial data
        let mut welcome_found = false;
        if !initial_data.is_empty() {
            if let Ok(data_str) = std::str::from_utf8(&initial_data) {
                if data_str.contains("Welcome") || data_str.contains("VSC:") {
                    welcome_found = true;
                    println!("Already found welcome message in initial data");
                }
            }
        }

        // If we haven't found a welcome message yet, try to receive it from the message channel
        if !welcome_found {
            println!("No welcome message found in initial data, trying to receive from message channel");

            // Receive the welcome message from the remote server with a timeout
            match time::timeout(
                Duration::from_secs(10), // Shorter timeout since we've already waited
                channel.receive()
            ).await {
                Ok(Ok(welcome)) => {
                    // Check if the welcome message contains "Welcome"
                    let welcome_str = String::from_utf8_lossy(&welcome);
                    if !welcome_str.contains("Welcome") {
                        println!("Received message doesn't contain 'Welcome': {}", welcome_str);
                        return Err(anyhow::anyhow!("Unexpected welcome message: {}", welcome_str));
                    }

                    println!("Received welcome message from message channel: {}", welcome_str);
                    welcome_found = true;
                },
                Ok(Err(e)) => {
                    println!("Error receiving welcome message from message channel: {}", e);
                    // If we couldn't receive a welcome message and didn't find one in initial data,
                    // that's an error
                    return Err(anyhow::anyhow!("Failed to receive welcome message: {}", e));
                },
                Err(e) => {
                    println!("Timeout receiving welcome message from message channel: {}", e);
                    // If we timed out and didn't find a welcome message in initial data,
                    // that's an error
                    return Err(anyhow::anyhow!("Timeout receiving welcome message: {}", e));
                }
            }
        }

        Ok((channel, welcome_found))
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
