use anyhow::Result;
use bytes::Bytes;
use clap::Parser;
use std::io::Write;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, error, info, warn};
use vscfreedev_core::message_channel::MessageChannel;
use vscfreedev_core::port_forward::{PortForwardManager, PortForwardMessage};

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
        // Test simple binary I/O directly without StdioAdapter
        debug!("REMOTE_SERVER_LOG: Starting with simple binary I/O test");
        handle_simple_binary_io().await?;
    } else {
        // Create a TCP listener
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", args.port)).await?;
        let (stream, _) = listener.accept().await?;
        let mut message_channel = MessageChannel::new(stream);
        handle_messages(&mut message_channel).await?;
    }

    Ok(())
}

async fn handle_messages<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    message_channel: &mut MessageChannel<T>,
) -> Result<()> {
    debug!("REMOTE_SERVER_LOG: handle_messages starting");

    let port_forward_manager = Arc::new(PortForwardManager::new());

    for i in 0..50 {
        // Increased iterations to handle multiple port forward requests
        debug!(
            "REMOTE_SERVER_LOG: Waiting for message... (iteration {})",
            i
        );

        debug!("REMOTE_SERVER_LOG: About to call message_channel.receive()");
        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            message_channel.receive(),
        )
        .await
        {
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
                        handle_port_forward_message(port_forward_msg, &port_forward_manager).await;
                    let response_json = serde_json::to_string(&response)?;

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
                warn!("REMOTE_SERVER_ERROR: Timeout waiting for message");
                break;
            }
        }
    }

    Ok(())
}

async fn handle_port_forward_message(
    message: PortForwardMessage,
    manager: &Arc<PortForwardManager>,
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

/// Simple binary I/O test to check if the issue is with StdioAdapter
async fn handle_simple_binary_io() -> Result<()> {
    debug!("REMOTE_SERVER_LOG: handle_simple_binary_io starting");

    // Use tokio's async stdin/stdout directly
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    let mut buffer = [0u8; 1024];

    for i in 0..10 {
        debug!("REMOTE_SERVER_LOG: Iteration {}, waiting for data...", i);

        match tokio::time::timeout(std::time::Duration::from_secs(5), stdin.read(&mut buffer)).await
        {
            Ok(Ok(n)) => {
                debug!("REMOTE_SERVER_LOG: Read {} bytes from stdin", n);
                debug!(
                    "REMOTE_SERVER_LOG: Raw bytes: {:?}",
                    &buffer[..std::cmp::min(10, n)]
                );

                if n >= 2 {
                    let length = u16::from_be_bytes([buffer[0], buffer[1]]);
                    debug!("REMOTE_SERVER_LOG: Parsed length: {}", length);

                    if n >= 2 + length as usize {
                        let payload = &buffer[2..2 + length as usize];
                        let payload_str = String::from_utf8_lossy(payload);
                        debug!("REMOTE_SERVER_LOG: Payload: {:?}", payload_str);

                        // Send echo response
                        let response = format!("Echo: {}", payload_str);
                        let response_bytes = response.as_bytes();
                        let response_len = response_bytes.len() as u16;

                        debug!(
                            "REMOTE_SERVER_LOG: Sending echo response, length: {}",
                            response_len
                        );

                        stdout.write_u16(response_len).await?;
                        stdout.write_all(response_bytes).await?;
                        stdout.flush().await?;

                        debug!("REMOTE_SERVER_LOG: Echo response sent");
                        break;
                    } else {
                        warn!(
                            "REMOTE_SERVER_LOG: Incomplete message, expected {} more bytes",
                            (2 + length as usize) - n
                        );
                    }
                } else {
                    warn!("REMOTE_SERVER_LOG: Not enough bytes for length header");
                }
            }
            Ok(Err(e)) => {
                error!("REMOTE_SERVER_ERROR: Error reading from stdin: {}", e);
                break;
            }
            Err(_) => {
                debug!("REMOTE_SERVER_LOG: Timeout reading from stdin");
                continue;
            }
        }
    }

    Ok(())
}
