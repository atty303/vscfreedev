use anyhow::Result;
use bytes::Bytes;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf, Stdin, Stdout};
use tokio::sync::{RwLock, mpsc};
use tracing::{error, info, warn};

use yuha_core::message_channel::MessageChannel;
use yuha_core::protocol::{ResponseBuffer, YuhaRequest, YuhaResponse};
use yuha_core::{browser, clipboard};
use yuha_remote::ipc::{IpcClient, IpcCommand, IpcResponse, get_default_ipc_socket_path};

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

/// Simplified remote server using request-response protocol
pub struct RemoteServer<T> {
    message_channel: MessageChannel<T>,
    response_buffer: Arc<RwLock<ResponseBuffer>>,
    active_connections: Arc<RwLock<HashMap<u32, mpsc::UnboundedSender<Bytes>>>>,
    next_connection_id: Arc<RwLock<u32>>,
}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> RemoteServer<T> {
    pub fn new(message_channel: MessageChannel<T>) -> Self {
        Self {
            message_channel,
            response_buffer: Arc::new(RwLock::new(ResponseBuffer::new())),
            active_connections: Arc::new(RwLock::new(HashMap::new())),
            next_connection_id: Arc::new(RwLock::new(1)),
        }
    }

    pub fn get_response_buffer(&self) -> Arc<RwLock<ResponseBuffer>> {
        self.response_buffer.clone()
    }

    /// Run the server main loop
    pub async fn run(&mut self) -> Result<()> {
        info!("Remote server starting with simple request-response protocol");

        loop {
            match self.message_channel.receive_request().await {
                Ok(request) => {
                    let response = self.handle_request(request).await;

                    if let Err(e) = self.message_channel.send_response(&response).await {
                        error!("Failed to send response: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Error receiving request: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Run the server with IPC support
    pub async fn run_with_ipc(&mut self, _ipc_sender: mpsc::UnboundedSender<String>) -> Result<()> {
        info!("Remote server starting with simple request-response protocol and IPC");

        // Channel to receive messages from IPC
        let (_tx, mut _rx) = mpsc::unbounded_channel::<String>();

        // Start IPC message handler
        let response_buffer = self.response_buffer.clone();
        tokio::spawn(async move {
            while let Some(message) = _rx.recv().await {
                info!("Received IPC message: {}", message);
                let mut buffer = response_buffer.write().await;
                buffer.add_clipboard_content(format!("IPC: {}", message));
            }
        });

        loop {
            tokio::select! {
                // Handle client requests
                request_result = self.message_channel.receive_request() => {
                    match request_result {
                        Ok(request) => {
                            let response = self.handle_request(request).await;

                            if let Err(e) = self.message_channel.send_response(&response).await {
                                error!("Failed to send response: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Error receiving request: {}", e);
                            break;
                        }
                    }
                }
                // Handle IPC messages (currently just logged)
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // Periodic check - could be used for other maintenance tasks
                }
            }
        }

        Ok(())
    }

    /// Handle a single request
    async fn handle_request(&mut self, request: YuhaRequest) -> YuhaResponse {
        match request {
            YuhaRequest::PollData => {
                let mut buffer = self.response_buffer.write().await;
                let items = buffer.take_items();
                if items.is_empty() {
                    // Long polling: wait for data
                    drop(buffer);
                    self.wait_for_data().await;
                    let mut buffer = self.response_buffer.write().await;
                    let items = buffer.take_items();
                    YuhaResponse::Data { items }
                } else {
                    YuhaResponse::Data { items }
                }
            }
            YuhaRequest::StartPortForward {
                local_port,
                remote_host,
                remote_port,
            } => {
                self.start_port_forward(local_port, remote_host, remote_port)
                    .await
            }
            YuhaRequest::StopPortForward { local_port } => self.stop_port_forward(local_port).await,
            YuhaRequest::PortForwardData {
                connection_id,
                data,
            } => self.forward_data(connection_id, data).await,
            YuhaRequest::GetClipboard => self.get_clipboard().await,
            YuhaRequest::SetClipboard { content } => self.set_clipboard(content).await,
            YuhaRequest::OpenBrowser { url } => self.open_browser(url).await,
        }
    }

    /// Wait for data with timeout for long polling
    async fn wait_for_data(&self) {
        // Long polling: wait up to 5 seconds for data
        // This balances low latency with reasonable timeout behavior
        const LONG_POLL_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(5);
        const POLL_INTERVAL: tokio::time::Duration = tokio::time::Duration::from_millis(10);

        let deadline = tokio::time::Instant::now() + LONG_POLL_TIMEOUT;

        while tokio::time::Instant::now() < deadline {
            {
                let buffer = self.response_buffer.read().await;
                if buffer.has_data() {
                    return;
                }
            }
            // Sleep briefly to avoid excessive lock contention
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        // Timeout is normal for long polling - client will retry
    }

    /// Start port forwarding
    async fn start_port_forward(
        &self,
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    ) -> YuhaResponse {
        info!(
            "Starting port forward: {} -> {}:{}",
            local_port, remote_host, remote_port
        );

        // Start a TCP listener for this port
        let listener_addr = format!("0.0.0.0:{}", local_port);
        match tokio::net::TcpListener::bind(&listener_addr).await {
            Ok(listener) => {
                let response_buffer = self.response_buffer.clone();
                let active_connections = self.active_connections.clone();
                let next_connection_id = self.next_connection_id.clone();

                // Spawn task to handle incoming connections
                tokio::spawn(async move {
                    loop {
                        match listener.accept().await {
                            Ok((client_stream, addr)) => {
                                let connection_id = {
                                    let mut id = next_connection_id.write().await;
                                    let current = *id;
                                    *id += 1;
                                    current
                                };

                                info!(
                                    "New connection {} from {} on port {}",
                                    connection_id, addr, local_port
                                );

                                let target_addr = format!("{}:{}", remote_host, remote_port);
                                let response_buffer_clone = response_buffer.clone();
                                let active_connections_clone = active_connections.clone();

                                // Handle each connection in a separate task
                                tokio::spawn(async move {
                                    // Notify client of new connection
                                    {
                                        let mut buffer = response_buffer_clone.write().await;
                                        buffer.add_new_connection(connection_id, local_port);
                                    }

                                    // Connect to target server
                                    match tokio::net::TcpStream::connect(&target_addr).await {
                                        Ok(target_stream) => {
                                            let (tx, rx) = mpsc::unbounded_channel();
                                            {
                                                let mut connections =
                                                    active_connections_clone.write().await;
                                                connections.insert(connection_id, tx);
                                            }

                                            Self::handle_connection(
                                                client_stream,
                                                target_stream,
                                                connection_id,
                                                response_buffer_clone,
                                                active_connections_clone,
                                                rx,
                                            )
                                            .await;
                                        }
                                        Err(e) => {
                                            error!("Failed to connect to {}: {}", target_addr, e);
                                            let mut buffer = response_buffer_clone.write().await;
                                            buffer.add_close_connection(connection_id);
                                        }
                                    }
                                });
                            }
                            Err(e) => {
                                error!("Error accepting connection: {}", e);
                                break;
                            }
                        }
                    }
                });

                YuhaResponse::Success
            }
            Err(e) => YuhaResponse::Error {
                message: format!("Failed to bind to {}: {}", listener_addr, e),
            },
        }
    }

    /// Stop port forwarding
    async fn stop_port_forward(&self, local_port: u16) -> YuhaResponse {
        info!("Stopping port forward for port {}", local_port);

        // Close all connections for this port
        let mut connections = self.active_connections.write().await;
        let mut to_remove = Vec::new();
        for (&connection_id, _) in connections.iter() {
            to_remove.push(connection_id);
        }

        for connection_id in to_remove {
            connections.remove(&connection_id);
            let mut buffer = self.response_buffer.write().await;
            buffer.add_close_connection(connection_id);
        }

        YuhaResponse::Success
    }

    /// Forward data to connection
    async fn forward_data(&self, connection_id: u32, data: Bytes) -> YuhaResponse {
        let connections = self.active_connections.read().await;
        if let Some(sender) = connections.get(&connection_id) {
            if let Err(e) = sender.send(data) {
                warn!("Failed to send data to connection {}: {}", connection_id, e);
                return YuhaResponse::Error {
                    message: format!("Failed to send data: {}", e),
                };
            }
        }
        YuhaResponse::Success
    }

    /// Get clipboard content
    async fn get_clipboard(&self) -> YuhaResponse {
        match clipboard::get_clipboard() {
            Ok(content) => {
                let mut buffer = self.response_buffer.write().await;
                buffer.add_clipboard_content(content);
                let items = buffer.take_items();
                YuhaResponse::Data { items }
            }
            Err(e) => YuhaResponse::Error {
                message: format!("Failed to get clipboard: {}", e),
            },
        }
    }

    /// Set clipboard content
    async fn set_clipboard(&self, content: String) -> YuhaResponse {
        match clipboard::set_clipboard(&content) {
            Ok(()) => YuhaResponse::Success,
            Err(e) => YuhaResponse::Error {
                message: format!("Failed to set clipboard: {}", e),
            },
        }
    }

    /// Open browser with URL
    async fn open_browser(&self, url: String) -> YuhaResponse {
        match browser::open_url(&url).await {
            Ok(()) => YuhaResponse::Success,
            Err(e) => YuhaResponse::Error {
                message: format!("Failed to open browser: {}", e),
            },
        }
    }

    /// Handle a single connection between client and target server
    async fn handle_connection(
        mut client_stream: tokio::net::TcpStream,
        mut target_stream: tokio::net::TcpStream,
        connection_id: u32,
        response_buffer: Arc<RwLock<ResponseBuffer>>,
        active_connections: Arc<RwLock<HashMap<u32, mpsc::UnboundedSender<Bytes>>>>,
        mut data_rx: mpsc::UnboundedReceiver<Bytes>,
    ) {
        let mut client_buf = [0; 4096];
        let mut target_buf = [0; 4096];

        loop {
            tokio::select! {
                // Read from client and write to target
                result = client_stream.read(&mut client_buf) => {
                    match result {
                        Ok(0) => {
                            break;
                        }
                        Ok(n) => {
                            if let Err(e) = target_stream.write_all(&client_buf[..n]).await {
                                error!("Error writing to target for connection {}: {}", connection_id, e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Error reading from client for connection {}: {}", connection_id, e);
                            break;
                        }
                    }
                }
                // Read from target and write to client
                result = target_stream.read(&mut target_buf) => {
                    match result {
                        Ok(0) => {
                            break;
                        }
                        Ok(n) => {
                            if let Err(e) = client_stream.write_all(&target_buf[..n]).await {
                                error!("Error writing to client for connection {}: {}", connection_id, e);
                                break;
                            }
                            // Also buffer data for client polling
                            {
                                let mut buffer = response_buffer.write().await;
                                buffer.add_port_forward_data(connection_id, Bytes::copy_from_slice(&target_buf[..n]));
                            }
                        }
                        Err(e) => {
                            error!("Error reading from target for connection {}: {}", connection_id, e);
                            break;
                        }
                    }
                }
                // Handle data from client via request
                data = data_rx.recv() => {
                    match data {
                        Some(data) => {
                            if let Err(e) = target_stream.write_all(&data).await {
                                error!("Error writing client data to target for connection {}: {}", connection_id, e);
                                break;
                            }
                        }
                        None => {
                            break;
                        }
                    }
                }
            }
        }

        // Clean up
        {
            let mut connections = active_connections.write().await;
            connections.remove(&connection_id);
        }

        {
            let mut buffer = response_buffer.write().await;
            buffer.add_close_connection(connection_id);
        }

        info!("Connection {} closed", connection_id);
    }
}

/// Remote server for yuha
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "9999")]
    port: u16,

    /// Use standard I/O instead of TCP
    #[arg(short, long)]
    stdio: bool,

    /// IPC socket path
    #[arg(long)]
    ipc_socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

/// Shell commands that can be executed directly
#[derive(Subcommand, Debug)]
enum Commands {
    /// Get clipboard content
    GetClipboard,
    /// Set clipboard content
    SetClipboard {
        /// Content to set
        content: String,
    },
    /// Open URL in browser
    OpenBrowser {
        /// URL to open
        url: String,
    },
    /// Send message to connected client
    SendToClient {
        /// Message to send
        message: String,
    },
    /// Get status of remote process
    Status,
    /// Ping the remote process
    Ping,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Write a simple message to a file as soon as the server starts
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
        error!("REMOTE_SERVER_ERROR: Failed to write to startup file");
    }

    info!("REMOTE_SERVER_STARTUP: Remote server process starting");

    // Try to log any panics to stderr
    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = std::backtrace::Backtrace::capture();
        let panic_msg = format!("PANIC: {:?}\nBacktrace: {}", panic_info, backtrace);

        error!("REMOTE_SERVER_ERROR: {}", panic_msg);

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

    // Check if this is a shell command execution
    if let Some(command) = args.command {
        return handle_shell_command(command, args.ipc_socket).await;
    }

    // Start transport server mode (called by client)
    let ipc_socket_path = args.ipc_socket.unwrap_or_else(get_default_ipc_socket_path);

    if args.stdio {
        info!("Starting yuha remote server using standard I/O with simple protocol and IPC");
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let stdio_stream = StdioStream::new(stdin, stdout);
        let message_channel = MessageChannel::new_with_stream(stdio_stream);
        let mut server = RemoteServer::new(message_channel);

        // Start IPC server in background with client communication
        let response_buffer = server.get_response_buffer();
        let (ipc_tx, _ipc_rx) = mpsc::unbounded_channel::<String>();
        let mut ipc_server =
            yuha_remote::ipc::IpcServer::new(ipc_socket_path.clone(), response_buffer);
        ipc_server.set_client_sender(ipc_tx.clone());

        tokio::spawn(async move {
            if let Err(e) = ipc_server.start().await {
                error!("IPC server error: {}", e);
            }
        });

        server.run_with_ipc(ipc_tx).await?;
    } else {
        info!(
            "Starting yuha remote server using TCP on port {} with simple protocol and IPC",
            args.port
        );
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", args.port)).await?;
        let (stream, _) = listener.accept().await?;
        let message_channel = MessageChannel::new(stream);
        let mut server = RemoteServer::new(message_channel);

        // Start IPC server in background with client communication
        let response_buffer = server.get_response_buffer();
        let (ipc_tx, _ipc_rx) = mpsc::unbounded_channel::<String>();
        let mut ipc_server =
            yuha_remote::ipc::IpcServer::new(ipc_socket_path.clone(), response_buffer);
        ipc_server.set_client_sender(ipc_tx.clone());

        tokio::spawn(async move {
            if let Err(e) = ipc_server.start().await {
                error!("IPC server error: {}", e);
            }
        });

        server.run_with_ipc(ipc_tx).await?;
    }

    Ok(())
}

/// Handle shell command execution
async fn handle_shell_command(command: Commands, ipc_socket: Option<PathBuf>) -> Result<()> {
    let socket_path = ipc_socket.unwrap_or_else(get_default_ipc_socket_path);
    let client = IpcClient::new(socket_path);

    let ipc_command = match command {
        Commands::GetClipboard => IpcCommand::GetClipboard,
        Commands::SetClipboard { content } => IpcCommand::SetClipboard { content },
        Commands::OpenBrowser { url } => IpcCommand::OpenBrowser { url },
        Commands::SendToClient { message } => IpcCommand::SendToClient { message },
        Commands::Status => IpcCommand::Status,
        Commands::Ping => IpcCommand::Ping,
    };

    match client.send_command(ipc_command).await {
        Ok(response) => match response {
            IpcResponse::Success { data } => {
                if let Some(data) = data {
                    println!("{}", data);
                } else {
                    println!("Success");
                }
            }
            IpcResponse::Error { message } => {
                eprintln!("Error: {}", message);
                std::process::exit(1);
            }
            IpcResponse::Status {
                uptime,
                connected_clients,
                active_port_forwards,
            } => {
                println!("Remote process status:");
                println!("  Uptime: {} seconds", uptime);
                println!("  Connected clients: {}", connected_clients);
                println!("  Active port forwards: {}", active_port_forwards);
            }
            IpcResponse::Pong => {
                println!("Pong");
            }
        },
        Err(e) => {
            eprintln!("Failed to communicate with remote process: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
