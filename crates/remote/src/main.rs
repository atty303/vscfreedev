use anyhow::Result;
use bytes::Bytes;
use clap::Parser;
use std::collections::HashMap;
use std::io::Write;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf, Stdin, Stdout};
use tokio::net::TcpStream;
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, error, info, warn};
use vscfreedev_core::message_channel::MessageChannel;
use vscfreedev_core::port_forward::{PortForwardManager, PortForwardMessage};

/// Type alias for active connections map
type ActiveConnections = Arc<RwLock<HashMap<(u16, u32), mpsc::UnboundedSender<Bytes>>>>;

/// A stream that combines stdin and stdout for bidirectional communication
pub struct StdioStream {
    stdin: Stdin,
    stdout: Stdout,
}

impl StdioStream {
    pub fn new(stdin: Stdin, stdout: Stdout) -> Self {
        Self { stdin, stdout }
    }
}

impl AsyncRead for StdioStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdin).poll_read(cx, buf)
    }
}

impl AsyncWrite for StdioStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stdout).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdout).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdout).poll_shutdown(cx)
    }
}

/// Remote server for vscfreedev
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "9999")]
    port: u16,

    /// Use standard I/O instead of TCP
    #[arg(short, long)]
    stdio: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    // Write a simple message to a file as soon as the server starts
    // This will help us determine if the server is being executed correctly
    let startup_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("/tmp/remote_startup.txt");

    if let Ok(mut file) = startup_file {
        let _ = writeln!(
            file,
            "Remote server started at {}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    } else {
        // If we can't write to the file, try to write to stderr
        error!("REMOTE_SERVER_ERROR: Failed to write to startup file");
    }

    // Log startup to stderr for better visibility in SSH logs
    info!("REMOTE_SERVER_STARTUP: Remote server process starting");

    // Try to log any panics to stderr
    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = std::backtrace::Backtrace::capture();
        let panic_msg = format!("PANIC: {:?}\nBacktrace: {}", panic_info, backtrace);

        // Log to stderr
        error!("REMOTE_SERVER_ERROR: {}", panic_msg);

        // Also try to write to a file
        let panic_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("/tmp/remote_panic.txt");

        if let Ok(mut file) = panic_file {
            let _ = writeln!(file, "{}", panic_msg);
        }
    }));

    let args = Args::parse();

    // Log startup to stderr for better visibility in SSH logs
    if args.stdio {
        info!("Starting vscfreedev remote server using standard I/O");
    } else {
        info!(
            "Starting vscfreedev remote server using TCP on port {}",
            args.port
        );
    }

    if args.stdio {
        // Create a stdio-based message channel for port forwarding
        debug!("REMOTE_SERVER_LOG: Starting with stdio-based message channel");

        // Create a combined stdin/stdout stream
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let stdio_stream = StdioStream::new(stdin, stdout);
        let mut message_channel = MessageChannel::new_with_stream(stdio_stream);
        handle_messages(&mut message_channel).await?;
    } else {
        // Create a TCP listener
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", args.port)).await?;
        let (stream, _) = listener.accept().await?;
        let mut message_channel = MessageChannel::new(stream);
        handle_messages(&mut message_channel).await?;
    }

    Ok(())
}

async fn handle_messages<
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
>(
    message_channel: &mut MessageChannel<T>,
) -> Result<()> {
    debug!("REMOTE_SERVER_LOG: handle_messages starting");

    let port_forward_manager = Arc::new(PortForwardManager::new());
    // Track active connections: (local_port, connection_id) -> sender
    let active_connections: ActiveConnections = Arc::new(RwLock::new(HashMap::new()));

    // Create a channel for sending responses back to client
    let (response_tx, mut response_rx) = mpsc::unbounded_channel::<PortForwardMessage>();

    // Handle both incoming messages and outgoing responses
    loop {
        tokio::select! {
            // Handle incoming messages from client
            result = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                message_channel.receive(),
            ) => {
                match result {
            Ok(Ok(message)) => {
                debug!("REMOTE_SERVER_LOG: Message received successfully");
                let message_str = String::from_utf8_lossy(&message);
                info!("Received message: {}", message_str);

                // Try to parse as port forward message first
                if let Ok(port_forward_msg) =
                    serde_json::from_str::<PortForwardMessage>(&message_str)
                {
                    debug!(
                        "REMOTE_SERVER_LOG: Parsed as port forward message: {:?}",
                        port_forward_msg
                    );

                    let response =
                        handle_port_forward_message(port_forward_msg, &port_forward_manager, &active_connections, &response_tx).await;
                    let response_json = serde_json::to_string(&response)?;

                    // Only send response for control messages (not data/connection messages)
                    match response {
                        PortForwardMessage::StartResponse { .. } |
                        PortForwardMessage::StopResponse { .. } => {
                            debug!(
                                "REMOTE_SERVER_LOG: Sending port forward response: {}",
                                response_json
                            );
                            match message_channel.send(Bytes::from(response_json)).await {
                                Ok(_) => {
                                    debug!("REMOTE_SERVER_LOG: Port forward response sent successfully");
                                }
                                Err(e) => {
                                    error!(
                                        "REMOTE_SERVER_ERROR: Failed to send port forward response: {}",
                                        e
                                    );
                                    return Err(anyhow::anyhow!(
                                        "Failed to send port forward response: {}",
                                        e
                                    ));
                                }
                            }
                        }
                        _ => {
                            // For data/connection messages, don't send explicit response
                            debug!("REMOTE_SERVER_LOG: Handled data/connection message");
                        }
                    }
                } else {
                    // Handle as regular echo message for backward compatibility
                    let echo_msg = format!("Echo: {}", message_str);
                    debug!("REMOTE_SERVER_LOG: Preparing to send echo: {}", echo_msg);

                    match message_channel.send(bytes::Bytes::from(echo_msg)).await {
                        Ok(_) => {
                            debug!("REMOTE_SERVER_LOG: Echo sent successfully");
                        }
                        Err(e) => {
                            error!("REMOTE_SERVER_ERROR: Failed to send echo: {}", e);
                            return Err(anyhow::anyhow!("Failed to send echo: {}", e));
                        }
                    }
                }
            }
                    Ok(Err(e)) => {
                        error!("REMOTE_SERVER_ERROR: Error receiving message: {}", e);
                        break;
                    }
                    Err(_timeout) => {
                        debug!("REMOTE_SERVER_LOG: Timeout waiting for message, continuing...");
                        continue;
                    }
                }
            }
            // Handle outgoing responses from port forwarding connections
            response = response_rx.recv() => {
                if let Some(response_msg) = response {
                    if let Ok(response_json) = serde_json::to_string(&response_msg) {
                        debug!("REMOTE_SERVER_LOG: Sending async response: {}", response_json);
                        if let Err(e) = message_channel.send(Bytes::from(response_json)).await {
                            error!("REMOTE_SERVER_ERROR: Failed to send async response: {}", e);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_port_forward_message(
    message: PortForwardMessage,
    manager: &Arc<PortForwardManager>,
    active_connections: &ActiveConnections,
    response_tx: &mpsc::UnboundedSender<PortForwardMessage>,
) -> PortForwardMessage {
    match message {
        PortForwardMessage::StartRequest {
            local_port,
            remote_host,
            remote_port,
        } => {
            info!(
                "Starting port forward: {} -> {}:{}",
                local_port, remote_host, remote_port
            );

            // For remote side, we actually want to connect to the local service
            // and forward it back to the client's local port
            match manager
                .start_forward(local_port, remote_host.clone(), remote_port)
                .await
            {
                Ok(_) => {
                    info!(
                        "Port forward started successfully: {} -> {}:{}",
                        local_port, remote_host, remote_port
                    );
                    PortForwardMessage::StartResponse {
                        local_port,
                        success: true,
                        error: None,
                    }
                }
                Err(e) => {
                    error!("Failed to start port forward: {}", e);
                    PortForwardMessage::StartResponse {
                        local_port,
                        success: false,
                        error: Some(e.to_string()),
                    }
                }
            }
        }
        PortForwardMessage::StopRequest { local_port } => {
            info!("Stopping port forward for port {}", local_port);

            // Clean up all connections for this port
            {
                let mut connections = active_connections.write().await;
                let keys_to_remove: Vec<_> = connections
                    .keys()
                    .filter(|(port, _)| *port == local_port)
                    .cloned()
                    .collect();

                for key in keys_to_remove {
                    connections.remove(&key);
                }
            }

            match manager.stop_forward(local_port).await {
                Ok(_) => {
                    info!("Port forward stopped successfully for port {}", local_port);
                    PortForwardMessage::StopResponse {
                        local_port,
                        success: true,
                        error: None,
                    }
                }
                Err(e) => {
                    error!("Failed to stop port forward: {}", e);
                    PortForwardMessage::StopResponse {
                        local_port,
                        success: false,
                        error: Some(e.to_string()),
                    }
                }
            }
        }
        PortForwardMessage::NewConnection {
            local_port,
            connection_id,
        } => {
            info!("New connection {} for port {}", connection_id, local_port);

            // Find the port forward info
            if let Some(port_forward) = manager.get_forward(local_port).await {
                let remote_host = port_forward.remote_host.clone();
                let remote_port = port_forward.remote_port;

                // Connect to the target server
                match TcpStream::connect((remote_host.as_str(), remote_port)).await {
                    Ok(mut stream) => {
                        info!(
                            "Connected to {}:{} for connection {}",
                            remote_host, remote_port, connection_id
                        );

                        let (tx, mut rx) = mpsc::unbounded_channel();

                        // Store the connection
                        {
                            let mut connections = active_connections.write().await;
                            connections.insert((local_port, connection_id), tx);
                        }

                        let active_connections_clone = active_connections.clone();
                        let response_tx_clone = response_tx.clone();

                        // Handle this remote connection
                        tokio::spawn(async move {
                            let mut buf = [0; 4096];
                            loop {
                                tokio::select! {
                                    // Read from target server and send to client
                                    result = stream.read(&mut buf) => {
                                        match result {
                                            Ok(0) => {
                                                debug!("Target server closed connection {}", connection_id);
                                                break;
                                            }
                                            Ok(n) => {
                                                let data_msg = PortForwardMessage::Data {
                                                    local_port,
                                                    connection_id,
                                                    data: buf[..n].to_vec(),
                                                };

                                                // Send data back to client
                                                if let Err(e) = response_tx_clone.send(data_msg) {
                                                    error!("Failed to send data response for connection {}: {}", connection_id, e);
                                                    break;
                                                }
                                                debug!("Sent {} bytes from target server to client for connection {}", n, connection_id);
                                            }
                                            Err(e) => {
                                                error!("Error reading from target server for connection {}: {}", connection_id, e);
                                                break;
                                            }
                                        }
                                    }
                                    // Receive data from client and write to target server
                                    data = rx.recv() => {
                                        match data {
                                            Some(data) => {
                                                if let Err(e) = stream.write_all(&data).await {
                                                    error!("Error writing to target server for connection {}: {}", connection_id, e);
                                                    break;
                                                }
                                            }
                                            None => {
                                                debug!("Data channel closed for connection {}", connection_id);
                                                break;
                                            }
                                        }
                                    }
                                }
                            }

                            // Clean up
                            {
                                let mut connections = active_connections_clone.write().await;
                                connections.remove(&(local_port, connection_id));
                            }

                            // Send close connection message to client
                            let close_msg = PortForwardMessage::CloseConnection {
                                local_port,
                                connection_id,
                            };
                            let _ = response_tx_clone.send(close_msg);
                        });
                    }
                    Err(e) => {
                        error!(
                            "Failed to connect to {}:{} for connection {}: {}",
                            remote_host, remote_port, connection_id, e
                        );
                    }
                }
            }

            // Return the original message (no response needed for NewConnection)
            message
        }
        PortForwardMessage::Data {
            local_port,
            connection_id,
            ref data,
        } => {
            debug!(
                "Received {} bytes for connection {} on port {}",
                data.len(),
                connection_id,
                local_port
            );

            // Forward data to the appropriate target connection
            {
                let connections = active_connections.read().await;
                if let Some(sender) = connections.get(&(local_port, connection_id)) {
                    if let Err(e) = sender.send(Bytes::from(data.clone())) {
                        warn!(
                            "Failed to send data to target connection {}: {}",
                            connection_id, e
                        );
                    }
                }
            }

            // Return the original message (no response needed for Data)
            message
        }
        PortForwardMessage::CloseConnection {
            local_port,
            connection_id,
        } => {
            info!(
                "Closing connection {} for port {}",
                connection_id, local_port
            );

            // Clean up the connection
            {
                let mut connections = active_connections.write().await;
                connections.remove(&(local_port, connection_id));
            }

            // Return the original message (no response needed for CloseConnection)
            message
        }
        // Other message types would be handled here for full implementation
        _ => {
            warn!("Unhandled port forward message type");
            PortForwardMessage::StartResponse {
                local_port: 0,
                success: false,
                error: Some("Unhandled message type".to_string()),
            }
        }
    }
}
