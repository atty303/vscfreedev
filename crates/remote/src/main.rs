use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use clap::Parser;
use std::io::{self, Read, Write};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use vscfreedev_core::message_channel::MessageChannel;

/// A simple async stdio adapter using tokio's async stdin/stdout
struct AsyncStdio {
    stdin: tokio::io::Stdin,
    stdout: tokio::io::Stdout,
}

impl AsyncStdio {
    fn new() -> Self {
        Self {
            stdin: tokio::io::stdin(),
            stdout: tokio::io::stdout(),
        }
    }
}

impl AsyncRead for AsyncStdio {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdin).poll_read(cx, buf)
    }
}

impl AsyncWrite for AsyncStdio {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stdout).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdout).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdout).poll_shutdown(cx)
    }
}

/// An adapter that implements AsyncRead and AsyncWrite for standard input and output
struct StdioAdapter {
    // Channel for reading from stdin
    read_rx: Receiver<io::Result<Vec<u8>>>,
    // Channel for writing to stdout
    write_tx: Sender<(Vec<u8>, Sender<io::Result<usize>>)>,
    // Buffer for data read from stdin
    read_buffer: Vec<u8>,
    // Waker for the task waiting for data
    waker: Option<Waker>,
    // Channel for receiving the result of a write operation
    write_result_rx: Option<Receiver<io::Result<usize>>>,
    // Data that was sent to the write thread
    write_data: Option<Vec<u8>>,
}

impl StdioAdapter {
    /// Create a new stdio adapter
    pub fn new() -> Self {
        // Create channels for reading from stdin and writing to stdout
        let (read_tx, read_rx) = channel::<io::Result<Vec<u8>>>(100);
        let (write_tx, mut write_rx) = channel::<(Vec<u8>, Sender<io::Result<usize>>)>(100);

        // Spawn a thread to read from stdin
        std::thread::spawn(move || {
            eprintln!("REMOTE_SERVER_LOG: StdioAdapter stdin thread starting");
            
            // Set stdin to non-blocking mode for better SSH exec compatibility
            #[cfg(unix)]
            {
                use std::os::unix::io::AsRawFd;
                let fd = io::stdin().as_raw_fd();
                unsafe {
                    let flags = libc::fcntl(fd, libc::F_GETFL);
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
            }
            
            let mut stdin = io::stdin();
            let mut buffer = [0u8; 1024];
            let mut loop_count = 0;

            loop {
                loop_count += 1;
                eprintln!("REMOTE_SERVER_LOG: StdioAdapter stdin thread loop {}", loop_count);
                
                // Read from stdin
                eprintln!("REMOTE_SERVER_LOG: About to call stdin.read()");
                match stdin.read(&mut buffer) {
                    Ok(0) => {
                        // EOF - in SSH exec mode, this might be temporary
                        eprintln!("REMOTE_SERVER_LOG: StdioAdapter read thread: EOF from stdin, retrying...");
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    }
                    Ok(n) => {
                        // Data read
                        eprintln!("REMOTE_SERVER_LOG: StdioAdapter read thread: Read {} bytes from stdin", n);
                        if read_tx.blocking_send(Ok(buffer[..n].to_vec())).is_err() {
                            // Channel closed, exit thread
                            break;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // No data available, sleep and try again
                        eprintln!("REMOTE_SERVER_LOG: stdin.read() returned WouldBlock, sleeping...");
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    }
                    Err(e) => {
                        // Error
                        eprintln!("REMOTE_SERVER_ERROR: StdioAdapter read thread: Error reading from stdin: {:?}", e);
                        if read_tx.blocking_send(Err(e)).is_err() {
                            // Channel closed, exit thread
                            break;
                        }
                    }
                }
            }
        });

        // Spawn a thread to write to stdout
        std::thread::spawn(move || {
            let mut stdout = io::stdout();

            while let Some((data, result_tx)) = write_rx.blocking_recv() {
                // Write to stdout
                let result = stdout.write(&data).and_then(|n| {
                    // Flush stdout after writing
                    stdout.flush()?;
                    Ok(n)
                });

                // Send the result back
                if result_tx.blocking_send(result).is_err() {
                    // Channel closed, but continue processing other writes
                    eprintln!("REMOTE_SERVER_ERROR: StdioAdapter write thread: Failed to send write result");
                }
            }
        });

        Self {
            read_rx,
            write_tx,
            read_buffer: Vec::new(),
            waker: None,
            write_result_rx: None,
            write_data: None,
        }
    }
}

impl AsyncRead for StdioAdapter {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Get a mutable reference to self
        let this = self.get_mut();

        // Store the waker for later use
        this.waker = Some(cx.waker().clone());

        // If we have data in the buffer, use it
        if !this.read_buffer.is_empty() {
            let unfilled = buf.initialize_unfilled();
            let n = std::cmp::min(unfilled.len(), this.read_buffer.len());
            unfilled[..n].copy_from_slice(&this.read_buffer[..n]);
            buf.advance(n);

            // Remove the used data from the buffer
            this.read_buffer = this.read_buffer.split_off(n);

            return Poll::Ready(Ok(()));
        }

        // Try to receive data from the channel
        match this.read_rx.try_recv() {
            Ok(Ok(data)) => {
                // Copy data to the buffer
                let unfilled = buf.initialize_unfilled();
                let n = std::cmp::min(unfilled.len(), data.len());
                unfilled[..n].copy_from_slice(&data[..n]);
                buf.advance(n);

                // Store any remaining data in the buffer
                if n < data.len() {
                    this.read_buffer = data[n..].to_vec();
                }

                Poll::Ready(Ok(()))
            }
            Ok(Err(e)) => {
                // Error received
                eprintln!("REMOTE_SERVER_ERROR: StdioAdapter::poll_read error from channel: {:?}", e);
                Poll::Ready(Err(e))
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                // No data available, yield and try again later
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                // Channel closed, likely due to EOF
                eprintln!("REMOTE_SERVER_LOG: StdioAdapter::poll_read channel closed due to EOF");
                Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "EOF from stdin",
                )))
            }
        }
    }
}

impl AsyncWrite for StdioAdapter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // Get a mutable reference to self
        let this = self.get_mut();

        // Store the waker for later use
        this.waker = Some(cx.waker().clone());

        // Check if we already have a result channel
        if let Some(result_rx) = &mut this.write_result_rx {
            // We already sent the data, now check if we have a result
            match result_rx.try_recv() {
                Ok(result) => {
                    // Result received, clear the result channel and data
                    this.write_result_rx = None;
                    this.write_data = None;

                    match result {
                        Ok(n) => {
                            Poll::Ready(Ok(n))
                        }
                        Err(e) => {
                            eprintln!("REMOTE_SERVER_ERROR: StdioAdapter::poll_write error: {:?}", e);
                            Poll::Ready(Err(e))
                        }
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    // No result yet, yield and try again later
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed, clear the result channel and data
                    this.write_result_rx = None;
                    this.write_data = None;

                    eprintln!("REMOTE_SERVER_ERROR: StdioAdapter::poll_write result channel closed");
                    Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "Write result channel closed",
                    )))
                }
            }
        } else {
            // We need to send the data
            // Check if the data has changed
            if this.write_data.as_ref().map_or(true, |data| data != buf) {
                // Data has changed, create a new channel and send the data
                let (result_tx, result_rx) = channel::<io::Result<usize>>(1);
                this.write_result_rx = Some(result_rx);
                this.write_data = Some(buf.to_vec());

                // Send the data to the write thread
                match this.write_tx.try_send((buf.to_vec(), result_tx)) {
                    Ok(()) => {
                        // Data sent, yield and try again later to check for the result
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        // Channel full, clear the result channel and data
                        this.write_result_rx = None;
                        this.write_data = None;

                        // Yield and try again later
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        // Channel closed, clear the result channel and data
                        this.write_result_rx = None;
                        this.write_data = None;

                        eprintln!("REMOTE_SERVER_ERROR: StdioAdapter::poll_write channel closed");
                        Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::BrokenPipe,
                            "Write channel closed",
                        )))
                    }
                }
            } else {
                // Data hasn't changed, this is a duplicate call
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        // Get a mutable reference to self
        let this = self.get_mut();

        // Store the waker for later use
        this.waker = Some(cx.waker().clone());

        // Check if we already have a result channel
        if let Some(result_rx) = &mut this.write_result_rx {
            // We already sent the data, now check if we have a result
            match result_rx.try_recv() {
                Ok(result) => {
                    // Result received, clear the result channel and data
                    this.write_result_rx = None;
                    this.write_data = None;

                    match result {
                        Ok(_) => {
                            Poll::Ready(Ok(()))
                        }
                        Err(e) => {
                            eprintln!("REMOTE_SERVER_ERROR: StdioAdapter::poll_flush error: {:?}", e);
                            Poll::Ready(Err(e))
                        }
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    // No result yet, yield and try again later
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed, clear the result channel and data
                    this.write_result_rx = None;
                    this.write_data = None;

                    eprintln!("REMOTE_SERVER_ERROR: StdioAdapter::poll_flush result channel closed");
                    Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "Flush result channel closed",
                    )))
                }
            }
        } else {
            // We need to send an empty buffer to flush stdout
            // Create a new channel and send the data
            let (result_tx, result_rx) = channel::<io::Result<usize>>(1);
            this.write_result_rx = Some(result_rx);
            this.write_data = Some(Vec::new()); // Empty buffer for flush

            // Send an empty buffer to the write thread, which will flush stdout after writing
            match this.write_tx.try_send((Vec::new(), result_tx)) {
                Ok(()) => {
                    // Data sent, yield and try again later to check for the result
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    // Channel full, clear the result channel and data
                    this.write_result_rx = None;
                    this.write_data = None;

                    // Yield and try again later
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    // Channel closed, clear the result channel and data
                    this.write_result_rx = None;
                    this.write_data = None;

                    eprintln!("REMOTE_SERVER_ERROR: StdioAdapter::poll_flush channel closed");
                    Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "Flush channel closed",
                    )))
                }
            }
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        // No need to do anything special for shutdown
        Poll::Ready(Ok(()))
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
    // Write a simple message to a file as soon as the server starts
    // This will help us determine if the server is being executed correctly
    let startup_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("/tmp/remote_startup.txt");

    if let Ok(mut file) = startup_file {
        let _ = writeln!(file, "Remote server started at {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
    } else {
        // If we can't write to the file, try to write to stderr
        eprintln!("REMOTE_SERVER_ERROR: Failed to write to startup file");
    }

    // Log startup to stderr for better visibility in SSH logs
    eprintln!("REMOTE_SERVER_STARTUP: Remote server process starting");

    // Try to log any panics to stderr
    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = std::backtrace::Backtrace::capture();
        let panic_msg = format!("PANIC: {:?}\nBacktrace: {}", panic_info, backtrace);

        // Log to stderr
        eprintln!("REMOTE_SERVER_ERROR: {}", panic_msg);

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
        eprintln!("Starting vscfreedev remote server using standard I/O");
    } else {
        eprintln!("Starting vscfreedev remote server using TCP on port {}", args.port);
    }

    if args.stdio {
        // Test simple binary I/O directly without StdioAdapter
        eprintln!("REMOTE_SERVER_LOG: Starting with simple binary I/O test");
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
    message_channel: &mut MessageChannel<T>
) -> Result<()> {
    eprintln!("REMOTE_SERVER_LOG: handle_messages starting");
    
    for i in 0..10 {  // Limit iterations to avoid infinite hang
        eprintln!("REMOTE_SERVER_LOG: Waiting for message... (iteration {})", i);

        eprintln!("REMOTE_SERVER_LOG: About to call message_channel.receive()");
        match tokio::time::timeout(std::time::Duration::from_secs(10), message_channel.receive()).await {
            Ok(Ok(message)) => {
                eprintln!("REMOTE_SERVER_LOG: Message received successfully");
                let message_str = String::from_utf8_lossy(&message);
                println!("Received message: {}", message_str);

                // Echo the message back
                let echo_msg = format!("Echo: {}", message_str);
                eprintln!("REMOTE_SERVER_LOG: Preparing to send echo: {}", echo_msg);

                match message_channel.send(bytes::Bytes::from(echo_msg)).await {
                    Ok(_) => {
                        eprintln!("REMOTE_SERVER_LOG: Echo sent successfully");
                    },
                    Err(e) => {
                        eprintln!("REMOTE_SERVER_ERROR: Failed to send echo: {}", e);
                        return Err(anyhow::anyhow!("Failed to send echo: {}", e));
                    }
                }
            }
            Ok(Err(e)) => {
                eprintln!("REMOTE_SERVER_ERROR: Error receiving message: {}", e);
                break;
            }
            Err(_timeout) => {
                eprintln!("REMOTE_SERVER_ERROR: Timeout waiting for message");
                break;
            }
        }
    }

    Ok(())
}

/// Simple binary I/O test to check if the issue is with StdioAdapter
async fn handle_simple_binary_io() -> Result<()> {
    eprintln!("REMOTE_SERVER_LOG: handle_simple_binary_io starting");
    
    // Use tokio's async stdin/stdout directly
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    
    let mut buffer = [0u8; 1024];
    
    for i in 0..10 {
        eprintln!("REMOTE_SERVER_LOG: Iteration {}, waiting for data...", i);
        
        match tokio::time::timeout(std::time::Duration::from_secs(5), stdin.read(&mut buffer)).await {
            Ok(Ok(n)) => {
                eprintln!("REMOTE_SERVER_LOG: Read {} bytes from stdin", n);
                eprintln!("REMOTE_SERVER_LOG: Raw bytes: {:?}", &buffer[..std::cmp::min(10, n)]);
                
                if n >= 2 {
                    let length = u16::from_be_bytes([buffer[0], buffer[1]]);
                    eprintln!("REMOTE_SERVER_LOG: Parsed length: {}", length);
                    
                    if n >= 2 + length as usize {
                        let payload = &buffer[2..2 + length as usize];
                        let payload_str = String::from_utf8_lossy(payload);
                        eprintln!("REMOTE_SERVER_LOG: Payload: {:?}", payload_str);
                        
                        // Send echo response
                        let response = format!("Echo: {}", payload_str);
                        let response_bytes = response.as_bytes();
                        let response_len = response_bytes.len() as u16;
                        
                        eprintln!("REMOTE_SERVER_LOG: Sending echo response, length: {}", response_len);
                        
                        stdout.write_u16(response_len).await?;
                        stdout.write_all(response_bytes).await?;
                        stdout.flush().await?;
                        
                        eprintln!("REMOTE_SERVER_LOG: Echo response sent");
                        break;
                    } else {
                        eprintln!("REMOTE_SERVER_LOG: Incomplete message, expected {} more bytes", (2 + length as usize) - n);
                    }
                } else {
                    eprintln!("REMOTE_SERVER_LOG: Not enough bytes for length header");
                }
            }
            Ok(Err(e)) => {
                eprintln!("REMOTE_SERVER_ERROR: Error reading from stdin: {}", e);
                break;
            }
            Err(_) => {
                eprintln!("REMOTE_SERVER_LOG: Timeout reading from stdin");
                continue;
            }
        }
    }
    
    Ok(())
}

/// Synchronous line-based handler for SSH exec mode compatibility
async fn handle_ssh_stdio() -> Result<()> {
    eprintln!("REMOTE_SERVER_LOG: handle_ssh_stdio starting with sync I/O");
    
    // Use blocking I/O in a spawn_blocking for SSH exec compatibility
    tokio::task::spawn_blocking(|| {
        use std::io::{BufRead, BufReader, Write};
        
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let reader = BufReader::new(stdin);
        
        eprintln!("REMOTE_SERVER_LOG: Starting to read lines from stdin (sync)");
        
        for (i, line_result) in reader.lines().enumerate() {
            if i >= 10 { break; } // Limit iterations
            
            eprintln!("REMOTE_SERVER_LOG: Reading line... (iteration {})", i);
            
            match line_result {
                Ok(line) => {
                    let trimmed = line.trim();
                    eprintln!("REMOTE_SERVER_LOG: Received line: {:?}", trimmed);
                    
                    if !trimmed.is_empty() {
                        // Parse VSC protocol message
                        if trimmed.starts_with("VSC:") && trimmed.len() > 8 {
                            let length_str = &trimmed[4..8];
                            let encoded_data = &trimmed[8..];
                            
                            eprintln!("REMOTE_SERVER_LOG: Parsing VSC message - length:{}, data:{}", length_str, encoded_data);
                            
                            if let Ok(decoded_bytes) = general_purpose::STANDARD.decode(encoded_data) {
                                if let Ok(decoded_str) = String::from_utf8(decoded_bytes.clone()) {
                                    let response = format!("Echo: {}", decoded_str.trim());
                                    eprintln!("REMOTE_SERVER_LOG: Sending response: {:?}", response);
                                    
                                    // Format as VSC protocol message  
                                    let response_bytes = response.as_bytes();
                                    let encoded_response = general_purpose::STANDARD.encode(response_bytes);
                                    let protocol_msg = format!("VSC:{:04X}{}\n", response_bytes.len(), encoded_response);
                                    eprintln!("REMOTE_SERVER_LOG: Sending protocol message: {:?}", protocol_msg.trim());
                                    
                                    if let Err(e) = stdout.write_all(protocol_msg.as_bytes()) {
                                        eprintln!("REMOTE_SERVER_ERROR: Error writing response: {}", e);
                                        break;
                                    }
                                    if let Err(e) = stdout.flush() {
                                        eprintln!("REMOTE_SERVER_ERROR: Error flushing: {}", e);
                                        break;
                                    }
                                    eprintln!("REMOTE_SERVER_LOG: Response sent successfully");
                                } else {
                                    eprintln!("REMOTE_SERVER_ERROR: Failed to decode UTF-8: {:?}", decoded_bytes);
                                }
                            } else {
                                eprintln!("REMOTE_SERVER_ERROR: Failed to decode base64: {}", encoded_data);
                            }
                        } else {
                            eprintln!("REMOTE_SERVER_ERROR: Invalid VSC protocol format: {}", trimmed);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("REMOTE_SERVER_ERROR: Error reading line: {}", e);
                    break;
                }
            }
        }
        
        eprintln!("REMOTE_SERVER_LOG: handle_ssh_stdio finished");
    }).await.map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?;
    
    Ok(())
}
