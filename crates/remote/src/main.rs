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
        let mut stdin = match self.stdin.lock() {
            Ok(stdin) => stdin,
            Err(_) => return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to lock stdin",
            ))),
        };

        let unfilled = buf.initialize_unfilled();
        match stdin.read(unfilled) {
            Ok(n) => {
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Standard I/O doesn't support async I/O, so we need to yield and try again later
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl AsyncWrite for StdioAdapter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut stdout = match self.stdout.lock() {
            Ok(stdout) => stdout,
            Err(_) => return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to lock stdout",
            ))),
        };

        match stdout.write(buf) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Standard I/O doesn't support async I/O, so we need to yield and try again later
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        let mut stdout = match self.stdout.lock() {
            Ok(stdout) => stdout,
            Err(_) => return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to lock stdout",
            ))),
        };

        match stdout.flush() {
            Ok(()) => Poll::Ready(Ok(())),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Standard I/O doesn't support async I/O, so we need to yield and try again later
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
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
    // No port needed anymore since we're using standard I/O
}

#[tokio::main]
async fn main() -> Result<()> {
    let _args = Args::parse();

    println!("Starting vscfreedev remote server using standard I/O");

    // Create a stdio adapter
    let stdio_adapter = StdioAdapter::new();

    // Create a message channel
    let message_channel = MessageChannel::new_with_stream(stdio_adapter);

    // Create a multiplexer
    let multiplexer = Multiplexer::new(message_channel);

    println!("Message channel established");

    // Create a channel for communication
    let mut channel = multiplexer.lock().await.create_channel().await?;

    // Send a welcome message
    channel.send(bytes::Bytes::from("Welcome! Remote channel established")).await?;

    // Keep the connection alive and handle messages
    loop {
        match channel.receive().await {
            Ok(message) => {
                let message_str = String::from_utf8_lossy(&message);
                println!("Received message: {}", message_str);

                // Echo the message back
                channel.send(bytes::Bytes::from(format!("Echo: {}", message_str))).await?;
            }
            Err(e) => {
                println!("Error receiving message: {}", e);
                break;
            }
        }
    }

    Ok(())
}
