use anyhow::Result;
use clap::Parser;
use std::io::{self, Read, Write};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use vscfreedev_core::message_channel::{MessageChannel, Multiplexer};

/// An adapter that implements AsyncRead and AsyncWrite for standard input and output
struct StdioAdapter {
    stdin: Arc<Mutex<io::Stdin>>,
    stdout: Arc<Mutex<io::Stdout>>,
}

impl StdioAdapter {
    /// Create a new stdio adapter
    pub fn new() -> Self {
        // Log that we're creating a new stdio adapter
        eprintln!("REMOTE_SERVER_LOG: Creating new StdioAdapter");

        Self {
            stdin: Arc::new(Mutex::new(io::stdin())),
            stdout: Arc::new(Mutex::new(io::stdout())),
        }
    }
}

impl AsyncRead for StdioAdapter {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Log that we're trying to read from stdin
        eprintln!("REMOTE_SERVER_LOG: StdioAdapter::poll_read called");

        let mut stdin = match self.stdin.lock() {
            Ok(stdin) => stdin,
            Err(e) => {
                eprintln!("REMOTE_SERVER_ERROR: Failed to lock stdin: {:?}", e);
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to lock stdin",
                )));
            }
        };

        let unfilled = buf.initialize_unfilled();
        match stdin.read(unfilled) {
            Ok(n) => {
                eprintln!("REMOTE_SERVER_LOG: StdioAdapter::poll_read read {} bytes", n);
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                eprintln!("REMOTE_SERVER_LOG: StdioAdapter::poll_read would block, yielding");
                // Standard I/O doesn't support async I/O, so we need to yield and try again later
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => {
                eprintln!("REMOTE_SERVER_ERROR: StdioAdapter::poll_read error: {:?}", e);
                Poll::Ready(Err(e))
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
        // Log that we're trying to write to stdout
        eprintln!("REMOTE_SERVER_LOG: StdioAdapter::poll_write called with {} bytes", buf.len());

        let mut stdout = match self.stdout.lock() {
            Ok(stdout) => stdout,
            Err(e) => {
                eprintln!("REMOTE_SERVER_ERROR: Failed to lock stdout: {:?}", e);
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to lock stdout",
                )));
            }
        };

        match stdout.write(buf) {
            Ok(n) => {
                eprintln!("REMOTE_SERVER_LOG: StdioAdapter::poll_write wrote {} bytes", n);
                Poll::Ready(Ok(n))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                eprintln!("REMOTE_SERVER_LOG: StdioAdapter::poll_write would block, yielding");
                // Standard I/O doesn't support async I/O, so we need to yield and try again later
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => {
                eprintln!("REMOTE_SERVER_ERROR: StdioAdapter::poll_write error: {:?}", e);
                Poll::Ready(Err(e))
            }
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        // Log that we're trying to flush stdout
        eprintln!("REMOTE_SERVER_LOG: StdioAdapter::poll_flush called");

        let mut stdout = match self.stdout.lock() {
            Ok(stdout) => stdout,
            Err(e) => {
                eprintln!("REMOTE_SERVER_ERROR: Failed to lock stdout for flush: {:?}", e);
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to lock stdout",
                )));
            }
        };

        match stdout.flush() {
            Ok(()) => {
                eprintln!("REMOTE_SERVER_LOG: StdioAdapter::poll_flush succeeded");
                Poll::Ready(Ok(()))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                eprintln!("REMOTE_SERVER_LOG: StdioAdapter::poll_flush would block, yielding");
                // Standard I/O doesn't support async I/O, so we need to yield and try again later
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => {
                eprintln!("REMOTE_SERVER_ERROR: StdioAdapter::poll_flush error: {:?}", e);
                Poll::Ready(Err(e))
            }
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        // Log that we're shutting down
        eprintln!("REMOTE_SERVER_LOG: StdioAdapter::poll_shutdown called");

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
    // Log to stderr immediately to see if the process is starting
    eprintln!("REMOTE_SERVER_STARTUP: Remote server process starting");

    // Write a simple message to a file as soon as the server starts
    // This will help us determine if the server is being executed correctly
    let startup_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("/tmp/remote_startup.txt");

    if let Ok(mut file) = startup_file {
        let _ = writeln!(file, "Remote server started at {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
        eprintln!("REMOTE_SERVER_STARTUP: Wrote to startup file");
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
        eprintln!("REMOTE_SERVER_LOG: Starting vscfreedev remote server using standard I/O");
        println!("Starting vscfreedev remote server using standard I/O");
    } else {
        eprintln!("REMOTE_SERVER_LOG: Starting vscfreedev remote server using TCP on port {}", args.port);
        println!("Starting vscfreedev remote server using TCP on port {}", args.port);
    }

    // Create a multiplexer based on the chosen transport
    eprintln!("REMOTE_SERVER_LOG: Creating message channel with transport: {}", if args.stdio { "stdio" } else { "tcp" });

    let multiplexer = if args.stdio {
        // Create a stdio adapter
        eprintln!("REMOTE_SERVER_LOG: Creating stdio adapter");
        let stdio_adapter = StdioAdapter::new();
        eprintln!("REMOTE_SERVER_LOG: Creating message channel with stdio adapter");
        let message_channel = MessageChannel::new_with_stream(stdio_adapter);
        eprintln!("REMOTE_SERVER_LOG: Creating multiplexer");
        Multiplexer::new(message_channel)
    } else {
        // Create a TCP listener
        eprintln!("REMOTE_SERVER_LOG: Creating TCP listener on port {}", args.port);
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", args.port)).await?;
        eprintln!("REMOTE_SERVER_LOG: Waiting for connection on port {}", args.port);
        let (stream, _) = listener.accept().await?;
        eprintln!("REMOTE_SERVER_LOG: Connection accepted from {}", stream.peer_addr()?);
        eprintln!("REMOTE_SERVER_LOG: Creating message channel with TCP stream");
        let message_channel = MessageChannel::new(stream);
        eprintln!("REMOTE_SERVER_LOG: Creating multiplexer");
        Multiplexer::new(message_channel)
    };

    eprintln!("REMOTE_SERVER_LOG: Multiplexer created");

    // Log message channel established
    let channel_msg = "Message channel established";
    println!("{}", channel_msg);
    eprintln!("REMOTE_SERVER_LOG: {}", channel_msg);

    // Create a channel for communication
    eprintln!("REMOTE_SERVER_LOG: Acquiring multiplexer lock");
    let mut multiplexer_lock = multiplexer.lock().await;
    eprintln!("REMOTE_SERVER_LOG: Multiplexer lock acquired, creating channel");

    let mut channel = match multiplexer_lock.create_channel().await {
        Ok(ch) => {
            eprintln!("REMOTE_SERVER_LOG: Channel created successfully");
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
    eprintln!("REMOTE_SERVER_LOG: {}", create_channel_msg);

    // Send a welcome message
    let welcome_msg = "Welcome! Remote channel established";
    eprintln!("REMOTE_SERVER_LOG: Preparing to send welcome message: {}", welcome_msg);

    match channel.send(bytes::Bytes::from(welcome_msg)).await {
        Ok(_) => {
            eprintln!("REMOTE_SERVER_LOG: Welcome message sent successfully");
        },
        Err(e) => {
            eprintln!("REMOTE_SERVER_ERROR: Failed to send welcome message: {}", e);
            return Err(anyhow::anyhow!("Failed to send welcome message: {}", e));
        }
    }

    // Log welcome message sent
    let welcome_sent_msg = "Welcome message sent";
    println!("{}", welcome_sent_msg);
    eprintln!("REMOTE_SERVER_LOG: {}", welcome_sent_msg);

    // Keep the connection alive and handle messages
    eprintln!("REMOTE_SERVER_LOG: Entering message handling loop");

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

    eprintln!("REMOTE_SERVER_LOG: Exiting message handling loop");

    Ok(())
}
