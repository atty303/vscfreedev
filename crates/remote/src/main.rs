use anyhow::Result;
use clap::Parser;
use std::net::TcpListener;
use tokio::net::TcpStream;
use vscfreedev_core::message_channel::{MessageChannel, Multiplexer};

/// Remote server for vscfreedev
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = 9999)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("Starting vscfreedev remote server on port {}", args.port);
    
    // Create a TCP listener
    let listener = TcpListener::bind(format!("127.0.0.1:{}", args.port))?;
    println!("Listening for connections...");

    // Accept a connection
    let (stream, addr) = listener.accept()?;
    println!("Connection accepted from: {}", addr);

    // Convert to tokio TcpStream
    let stream = TcpStream::from_std(stream)?;
    
    // Create a message channel
    let message_channel = MessageChannel::new(stream);
    
    // Create a multiplexer
    let multiplexer = Multiplexer::new(message_channel);
    
    println!("Message channel established");
    
    // Create a channel for communication
    let mut channel = multiplexer.lock().await.create_channel().await?;
    
    // Send a welcome message
    channel.send(bytes::Bytes::from("Remote channel established")).await?;
    
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