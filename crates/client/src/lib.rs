//! Client-side library for vscfreedev

use anyhow::Result;
use russh::client::{Config, Handler, Handle, Session, connect};
use russh::ChannelId;
use russh_keys::key::PublicKey;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc, Mutex};
use vscfreedev_core::message_channel::MessageChannel;

/// Error types for the client
#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("SSH error: {0}")]
    Ssh(#[from] russh::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Remote execution error: {0}")]
    RemoteExecution(String),

    #[error("Channel error: {0}")]
    Channel(String),

    #[error("Key error: {0}")]
    Key(#[from] russh_keys::Error),
}

/// Handler for SSH client events
struct MyHandler {
    data_tx: Arc<Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>>,
}

impl MyHandler {
    fn new() -> (Self, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                data_tx: Arc::new(Mutex::new(Some(tx))),
            },
            rx,
        )
    }
}

#[async_trait::async_trait]
impl Handler for MyHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        eprintln!("RusshHandler: Received {} bytes", data.len());
        let data_tx_guard = self.data_tx.lock().await;
        if let Some(ref tx) = *data_tx_guard {
            if let Err(e) = tx.send(data.to_vec()) {
                eprintln!("RusshHandler: Failed to send data: {}", e);
            }
        }
        Ok(())
    }
}

/// An adapter that implements AsyncRead and AsyncWrite for a russh channel
pub struct SshChannelAdapter {
    handle: Arc<Mutex<Handle<MyHandler>>>,
    channel_id: ChannelId,
    read_rx: Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
    read_buf: Arc<Mutex<Vec<u8>>>,
}

impl SshChannelAdapter {
    /// Create a new SSH channel adapter
    pub fn new(
        handle: Handle<MyHandler>,
        channel_id: ChannelId,
        read_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    ) -> Self {
        Self {
            handle: Arc::new(Mutex::new(handle)),
            channel_id,
            read_rx: Arc::new(Mutex::new(read_rx)),
            read_buf: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl AsyncRead for SshChannelAdapter {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // Try to get data from our buffer first
        if let Ok(mut buffer_guard) = self.read_buf.try_lock() {
            if !buffer_guard.is_empty() {
                let to_copy = std::cmp::min(buf.remaining(), buffer_guard.len());
                let data = buffer_guard.drain(..to_copy).collect::<Vec<u8>>();
                buf.put_slice(&data);
                eprintln!("SshChannelAdapter::poll_read returned {} bytes from buffer", to_copy);
                return Poll::Ready(Ok(()));
            }
        }

        // Try to receive new data
        if let Ok(mut rx_guard) = self.read_rx.try_lock() {
            match rx_guard.try_recv() {
                Ok(data) => {
                    eprintln!("SshChannelAdapter::poll_read got {} bytes", data.len());
                    let to_copy = std::cmp::min(buf.remaining(), data.len());
                    buf.put_slice(&data[..to_copy]);
                    
                    // Store remainder in buffer if any
                    if to_copy < data.len() {
                        if let Ok(mut buffer_guard) = self.read_buf.try_lock() {
                            *buffer_guard = data[to_copy..].to_vec();
                        }
                    }
                    
                    eprintln!("SshChannelAdapter::poll_read returned {} bytes", to_copy);
                    return Poll::Ready(Ok(()));
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    return Poll::Pending;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    eprintln!("SshChannelAdapter::poll_read channel disconnected");
                    return Poll::Ready(Ok(()));
                }
            }
        }

        Poll::Pending
    }
}

impl AsyncWrite for SshChannelAdapter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        eprintln!("SshChannelAdapter::poll_write called with {} bytes", buf.len());
        eprintln!("SshChannelAdapter::poll_write data: {}", String::from_utf8_lossy(buf));
        
        let handle = self.handle.clone();
        let channel_id = self.channel_id;
        let data = buf.to_vec();

        // Use futures::task::noop_waker to avoid borrowing issues
        let waker = futures::task::noop_waker();
        let mut context = std::task::Context::from_waker(&waker);

        // Attempt to send data immediately
        tokio::pin! {
            let send_fut = async move {
                let mut handle_guard = handle.lock().await;
                handle_guard.data(channel_id, data.clone().into()).await
                    .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "SSH data send failed"))
            };
        }

        match send_fut.poll(&mut context) {
            Poll::Ready(Ok(_)) => {
                eprintln!("SshChannelAdapter::poll_write sent {} bytes successfully", buf.len());
                Poll::Ready(Ok(buf.len()))
            }
            Poll::Ready(Err(e)) => {
                eprintln!("SshChannelAdapter::poll_write error: {}", e);
                Poll::Ready(Err(e))
            }
            Poll::Pending => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        eprintln!("SshChannelAdapter::poll_flush called");
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        eprintln!("SshChannelAdapter::poll_shutdown called");
        Poll::Ready(Ok(()))
    }
}

/// Public client interface
pub mod client {
    use super::*;

    /// Connect to a remote host via SSH and return a message channel
    pub async fn connect_ssh(
        host: &str,
        port: u16,
        username: &str,
        password: Option<&str>,
        key_path: Option<&Path>,
    ) -> Result<MessageChannel<SshChannelAdapter>, ClientError> {
        eprintln!("Connecting to {}:{} as {}", host, port, username);

        let config = Arc::new(Config::default());
        let (handler, data_rx) = MyHandler::new();

        // Connect to SSH server
        let mut handle = connect(config, (host, port), handler).await?;

        // Authenticate
        if let Some(password) = password {
            eprintln!("Authenticating with password");
            let auth_result = handle.authenticate_password(username, password).await?;
            if !auth_result {
                return Err(ClientError::Connection("Password authentication failed".to_string()));
            }
        } else if let Some(key_path) = key_path {
            eprintln!("Authenticating with key: {:?}", key_path);
            let key_str = std::fs::read_to_string(key_path)?;
            let key = russh_keys::decode_secret_key(&key_str, None)?;
            let auth_result = handle.authenticate_publickey(username, Arc::new(key)).await?;
            if !auth_result {
                return Err(ClientError::Connection("Key authentication failed".to_string()));
            }
        } else {
            return Err(ClientError::Connection("No authentication method provided".to_string()));
        }

        eprintln!("Authentication successful");

        // Create a channel
        let mut channel = handle.channel_open_session().await?;
        let channel_id = channel.id();
        eprintln!("Created SSH channel: {:?}", channel_id);

        // Execute the remote command
        let remote_path = "/usr/local/bin/vscfreedev_remote";
        let command = format!("{} -s 2>/tmp/remote_stderr.log", remote_path);
        eprintln!("Executing command: {}", command);

        // Execute the command using channel exec
        channel.exec(false, command.as_bytes()).await?;
        eprintln!("Command executed successfully");

        // Create the adapter
        let ssh_adapter = SshChannelAdapter::new(handle, channel_id, data_rx);

        // Create a message channel using text-safe mode for SSH compatibility  
        let message_channel = MessageChannel::new_with_text_safe_mode(ssh_adapter);

        Ok(message_channel)
    }
}