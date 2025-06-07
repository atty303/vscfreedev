use bytes::{Buf, Bytes, BytesMut};
use std::io;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

/// Error types for the message channel
#[derive(Error, Debug)]
pub enum ChannelError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Channel closed")]
    Closed,
}

/// Result type for message channel operations
pub type Result<T> = std::result::Result<T, ChannelError>;

/// A channel ID used to multiplex different protocols
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelId(pub u8);

/// A message sent over the channel
#[derive(Debug, Clone)]
pub struct Message {
    /// The channel ID this message belongs to
    pub channel_id: ChannelId,
    /// The message payload
    pub payload: Bytes,
}

/// A bidirectional message channel over a TCP connection
///
/// This channel allows multiplexing different protocols (SSH, HTTP, etc.)
/// over a single TCP connection with minimal overhead.
///
/// The wire format is:
/// - 1 byte: channel ID
/// - 2 bytes: payload length (big endian)
/// - N bytes: payload
pub struct MessageChannel<T> {
    inner: T,
    read_buffer: BytesMut,
}

impl MessageChannel<TcpStream> {
    /// Create a new message channel from a TCP stream
    pub fn new(stream: TcpStream) -> Self {
        Self {
            inner: stream,
            read_buffer: BytesMut::with_capacity(4096),
        }
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> MessageChannel<T> {
    /// Send a message over the channel
    pub async fn send(&mut self, message: Message) -> Result<()> {
        let Message {
            channel_id,
            payload,
        } = message;
        let payload_len = payload.len();

        if payload_len > u16::MAX as usize {
            return Err(ChannelError::Protocol(format!(
                "Payload too large: {}",
                payload_len
            )));
        }

        // Write channel ID (1 byte)
        self.inner.write_u8(channel_id.0).await?;

        // Write payload length (2 bytes, big endian)
        self.inner.write_u16(payload_len as u16).await?;

        // Write payload
        self.inner.write_all(&payload).await?;

        Ok(())
    }

    /// Receive a message from the channel
    pub async fn receive(&mut self) -> Result<Message> {
        loop {
            // Try to read a complete message from the buffer
            if self.read_buffer.len() >= 3 {
                // Read channel ID and payload length without advancing the cursor
                let channel_id = self.read_buffer[0];
                let payload_len =
                    u16::from_be_bytes([self.read_buffer[1], self.read_buffer[2]]) as usize;

                // Check if we have the complete message
                if self.read_buffer.len() >= 3 + payload_len {
                    // Advance the cursor past the header
                    self.read_buffer.advance(3);

                    // Extract the payload
                    let payload = self.read_buffer.split_to(payload_len).freeze();

                    return Ok(Message {
                        channel_id: ChannelId(channel_id),
                        payload,
                    });
                }
            }

            // Read more data into the buffer
            let bytes_read = self.inner.read_buf(&mut self.read_buffer).await?;
            if bytes_read == 0 {
                return Err(ChannelError::Closed);
            }
        }
    }
}

/// A handle to a specific channel within a message channel
pub struct ChannelHandle {
    channel_id: ChannelId,
    sender: tokio::sync::mpsc::Sender<Message>,
    receiver: tokio::sync::mpsc::Receiver<Message>,
}

impl ChannelHandle {
    /// Send data on this channel
    pub async fn send(&self, data: Bytes) -> Result<()> {
        self.sender
            .send(Message {
                channel_id: self.channel_id,
                payload: data,
            })
            .await
            .map_err(|_| ChannelError::Closed)
    }

    /// Receive data from this channel
    pub async fn receive(&mut self) -> Result<Bytes> {
        self.receiver
            .recv()
            .await
            .map(|msg| msg.payload)
            .ok_or(ChannelError::Closed)
    }
}

/// A multiplexer that manages multiple channels over a single message channel
pub struct Multiplexer {
    next_channel_id: u8,
    channels: std::collections::HashMap<ChannelId, tokio::sync::mpsc::Sender<Message>>,
    message_sender: tokio::sync::mpsc::Sender<Message>,
}

impl Multiplexer {
    /// Create a new multiplexer with the given message channel
    pub fn new<T: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        mut message_channel: MessageChannel<T>,
    ) -> Arc<tokio::sync::Mutex<Self>> {
        let (message_sender, mut message_receiver) = tokio::sync::mpsc::channel(100);

        let multiplexer = Arc::new(tokio::sync::Mutex::new(Self {
            next_channel_id: 1, // Start from 1, reserve 0 for control messages if needed
            channels: std::collections::HashMap::new(),
            message_sender,
        }));

        // Clone for the task
        let multiplexer_clone = multiplexer.clone();

        // Spawn a task to handle both sending and receiving messages
        tokio::spawn(async move {
            // Create a channel for outgoing messages to be processed
            let (outgoing_tx, mut outgoing_rx) = tokio::sync::mpsc::channel::<Message>(100);

            // Forward messages from the main channel to the outgoing channel
            tokio::spawn(async move {
                while let Some(message) = message_receiver.recv().await {
                    if outgoing_tx.send(message).await.is_err() {
                        break;
                    }
                }
            });

            // Process both incoming and outgoing messages
            loop {
                tokio::select! {
                    // Process outgoing messages
                    Some(message) = outgoing_rx.recv() => {
                        if let Err(e) = message_channel.send(message).await {
                            eprintln!("Error sending message: {}", e);
                            break;
                        }
                    }

                    // Process incoming messages
                    result = message_channel.receive() => {
                        match result {
                            Ok(message) => {
                                // Store the channel_id before moving the message
                                let channel_id = message.channel_id;

                                let mut multiplexer = multiplexer_clone.lock().await;
                                if let Some(sender) = multiplexer.channels.get(&channel_id) {
                                    if let Err(_) = sender.send(message).await {
                                        // Channel was closed, remove it
                                        multiplexer.channels.remove(&channel_id);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error receiving message: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        });

        multiplexer
    }

    /// Create a new channel
    pub async fn create_channel(&mut self) -> Result<ChannelHandle> {
        let channel_id = ChannelId(self.next_channel_id);
        self.next_channel_id = self.next_channel_id.wrapping_add(1);

        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        self.channels.insert(channel_id, sender.clone());

        Ok(ChannelHandle {
            channel_id,
            sender: self.message_sender.clone(),
            receiver,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn test_send_receive() {
        let (client, server) = duplex(1024);

        let mut client_channel = MessageChannel {
            inner: client,
            read_buffer: BytesMut::with_capacity(4096),
        };
        let mut server_channel = MessageChannel {
            inner: server,
            read_buffer: BytesMut::with_capacity(4096),
        };

        // Client sends a message
        let client_message = Message {
            channel_id: ChannelId(1),
            payload: Bytes::from_static(b"Hello, server!"),
        };

        tokio::spawn(async move {
            client_channel.send(client_message).await.unwrap();
        });

        // Server receives the message
        let received = server_channel.receive().await.unwrap();
        assert_eq!(received.channel_id.0, 1);
        assert_eq!(received.payload, Bytes::from_static(b"Hello, server!"));
    }

    #[tokio::test]
    async fn test_multiplexer() {
        let (client, server) = duplex(1024);

        let client_channel = MessageChannel {
            inner: client,
            read_buffer: BytesMut::with_capacity(4096),
        };
        let server_channel = MessageChannel {
            inner: server,
            read_buffer: BytesMut::with_capacity(4096),
        };

        let client_multiplexer = Multiplexer::new(client_channel);
        let server_multiplexer = Multiplexer::new(server_channel);

        // Create channels
        let client_http = client_multiplexer
            .lock()
            .await
            .create_channel()
            .await
            .unwrap();
        let client_ssh = client_multiplexer
            .lock()
            .await
            .create_channel()
            .await
            .unwrap();

        let mut server_channels = Vec::new();
        for _ in 0..2 {
            server_channels.push(
                server_multiplexer
                    .lock()
                    .await
                    .create_channel()
                    .await
                    .unwrap(),
            );
        }

        // Client sends messages on different channels
        client_http
            .send(Bytes::from_static(b"HTTP request"))
            .await
            .unwrap();
        client_ssh
            .send(Bytes::from_static(b"SSH data"))
            .await
            .unwrap();

        // Server receives messages
        let mut received_messages = Vec::new();
        for channel in &mut server_channels {
            if let Ok(msg) =
                tokio::time::timeout(std::time::Duration::from_millis(100), channel.receive()).await
            {
                received_messages.push(msg.unwrap());
            }
        }

        assert_eq!(received_messages.len(), 2);
        assert!(received_messages.contains(&Bytes::from_static(b"HTTP request")));
        assert!(received_messages.contains(&Bytes::from_static(b"SSH data")));
    }
}
