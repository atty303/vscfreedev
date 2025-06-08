use anyhow::Result;
use clap::Parser;
use std::io::{self, Read, Write};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use vscfreedev_core::message_channel::{MessageChannel, Multiplexer};

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
            let mut stdin = io::stdin();
            let mut buffer = [0u8; 1024];

            loop {
                // Read from stdin
                match stdin.read(&mut buffer) {
                    Ok(0) => {
                        // EOF
                        if read_tx.blocking_send(Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF from stdin"))).is_err() {
                            // Channel closed, exit thread
                            break;
                        }
                    }
                    Ok(n) => {
                        // Data read
                        if read_tx.blocking_send(Ok(buffer[..n].to_vec())).is_err() {
                            // Channel closed, exit thread
                            break;
                        }
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
        mut self: Pin<&mut Self>,
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
                // Channel closed
                eprintln!("REMOTE_SERVER_ERROR: StdioAdapter::poll_read channel closed");
                Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Read channel closed",
                )))
            }
        }
    }
}

impl AsyncWrite for StdioAdapter {
    fn poll_write(
        mut self: Pin<&mut Self>,
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
        mut self: Pin<&mut Self>,
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

    // Also write to stdout for visibility in SSH output
    println!("REMOTE_SERVER_STARTUP: Remote server process starting");

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
        println!("Starting vscfreedev remote server using standard I/O");
    } else {
        println!("Starting vscfreedev remote server using TCP on port {}", args.port);
    }

    // Create a multiplexer based on the chosen transport
    let multiplexer = if args.stdio {
        // Create a stdio adapter
        let stdio_adapter = StdioAdapter::new();
        let message_channel = MessageChannel::new_with_stream(stdio_adapter);
        Multiplexer::new(message_channel)
    } else {
        // Create a TCP listener
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", args.port)).await?;
        let (stream, _) = listener.accept().await?;
        let message_channel = MessageChannel::new(stream);
        Multiplexer::new(message_channel)
    };

    // Log message channel established
    let channel_msg = "Message channel established";
    println!("{}", channel_msg);

    // Create a channel for communication
    let mut multiplexer_lock = multiplexer.lock().await;

    let mut channel = match multiplexer_lock.create_channel().await {
        Ok(ch) => {
            ch
        },
        Err(e) => {
            eprintln!("REMOTE_SERVER_ERROR: Failed to create channel: {}", e);
            return Err(anyhow::anyhow!("Failed to create channel: {}", e));
        }
    };

    // Log channel created
    let create_channel_msg = "Communication channel created";
    println!("{}", create_channel_msg);

    // Send a welcome message
    let welcome_msg = "Welcome! Remote channel established";

    match channel.send(bytes::Bytes::from(welcome_msg)).await {
        Ok(_) => {
            // Welcome message sent successfully
        },
        Err(e) => {
            eprintln!("REMOTE_SERVER_ERROR: Failed to send welcome message: {}", e);
            return Err(anyhow::anyhow!("Failed to send welcome message: {}", e));
        }
    }

    // Log welcome message sent
    let welcome_sent_msg = "Welcome message sent";
    println!("{}", welcome_sent_msg);

    loop {
        // Log waiting for message
        let waiting_msg = "Waiting for message...";
        println!("{}", waiting_msg);
        eprintln!("REMOTE_SERVER_LOG: {}", waiting_msg);

        eprintln!("REMOTE_SERVER_LOG: Calling channel.receive()");
        match channel.receive().await {
            Ok(message) => {
                eprintln!("REMOTE_SERVER_LOG: Message received successfully");
                let message_str = String::from_utf8_lossy(&message);
                let received_msg = format!("Received message: {}", message_str);
                println!("{}", received_msg);
                eprintln!("REMOTE_SERVER_LOG: {}", received_msg);

                // Echo the message back
                let echo_msg = format!("Echo: {}", message_str);
                eprintln!("REMOTE_SERVER_LOG: Preparing to send echo: {}", echo_msg);

                match channel.send(bytes::Bytes::from(echo_msg)).await {
                    Ok(_) => {
                        eprintln!("REMOTE_SERVER_LOG: Echo sent successfully");
                    },
                    Err(e) => {
                        eprintln!("REMOTE_SERVER_ERROR: Failed to send echo: {}", e);
                        return Err(anyhow::anyhow!("Failed to send echo: {}", e));
                    }
                }
            }
            Err(e) => {
                let error_msg = format!("Error receiving message: {}", e);
                println!("{}", error_msg);
                eprintln!("REMOTE_SERVER_ERROR: {}", error_msg);
                break;
            }
        }
    }

    Ok(())
}
