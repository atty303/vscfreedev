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

/// Transport mode for the message channel
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransportMode {
    /// Raw binary mode for TCP connections
    Binary,
    /// Base64 text mode for SSH and other text-based transports
    TextSafe,
}

/// A bidirectional message channel over various transports
///
/// This channel allows multiplexing different protocols (SSH, HTTP, etc.)
/// over a single connection with transport-appropriate encoding.
///
/// Binary mode wire format:
/// - 1 byte: channel ID
/// - 2 bytes: payload length (big endian)
/// - N bytes: payload
///
/// Text-safe mode wire format:
/// - "VSC:" prefix
/// - 2 hex chars: channel ID
/// - 4 hex chars: payload length
/// - Base64 encoded payload
/// - "\n" suffix
pub struct MessageChannel<T> {
    inner: T,
    read_buffer: BytesMut,
    mode: TransportMode,
    text_line_buffer: String,
}

impl MessageChannel<TcpStream> {
    /// Create a new message channel from a TCP stream
    pub fn new(stream: TcpStream) -> Self {
        Self {
            inner: stream,
            read_buffer: BytesMut::with_capacity(4096),
            mode: TransportMode::Binary,
            text_line_buffer: String::new(),
        }
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> MessageChannel<T> {
    /// Create a new message channel from any stream that implements AsyncRead + AsyncWrite + Unpin
    pub fn new_with_stream(stream: T) -> Self {
        Self {
            inner: stream,
            read_buffer: BytesMut::with_capacity(4096),
            mode: TransportMode::Binary,
            text_line_buffer: String::new(),
        }
    }

    /// Create a new message channel with SSH-safe text mode
    pub fn new_with_text_safe_mode(stream: T) -> Self {
        Self {
            inner: stream,
            read_buffer: BytesMut::with_capacity(4096),
            mode: TransportMode::TextSafe,
            text_line_buffer: String::new(),
        }
    }
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

        match self.mode {
            TransportMode::Binary => {
                // Write channel ID (1 byte)
                self.inner.write_u8(channel_id.0).await?;

                // Write payload length (2 bytes, big endian)
                self.inner.write_u16(payload_len as u16).await?;

                // Write payload
                self.inner.write_all(&payload).await?;
            }
            TransportMode::TextSafe => {
                // Encode as text-safe format: VSC:CCLLLL<base64>\n
                let encoded_payload = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &payload);
                let frame = format!("VSC:{:02X}{:04X}{}\n", channel_id.0, payload_len, encoded_payload);
                self.inner.write_all(frame.as_bytes()).await?;
            }
        }

        // Explicitly flush the stream to ensure data is sent
        self.inner.flush().await?;

        Ok(())
    }

    /// Receive a message from the channel
    pub async fn receive(&mut self) -> Result<Message> {
        match self.mode {
            TransportMode::Binary => self.receive_binary().await,
            TransportMode::TextSafe => self.receive_text_safe().await,
        }
    }

    async fn receive_binary(&mut self) -> Result<Message> {
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

    async fn receive_text_safe(&mut self) -> Result<Message> {
        eprintln!("MSGCHAN: receive_text_safe starting");
        loop {
            // Read data into buffer and append to text line buffer
            eprintln!("MSGCHAN: trying to read from inner stream");
            let bytes_read = self.inner.read_buf(&mut self.read_buffer).await?;
            eprintln!("MSGCHAN: read {} bytes", bytes_read);
            
            if bytes_read == 0 {
                eprintln!("MSGCHAN: EOF on stream");
                return Err(ChannelError::Closed);
            }

            // Convert new bytes to string and append to line buffer
            let new_text = String::from_utf8_lossy(&self.read_buffer);
            eprintln!("MSGCHAN: new text received: {:?}", new_text);
            self.text_line_buffer.push_str(&new_text);
            self.read_buffer.clear();
            eprintln!("MSGCHAN: current line buffer: {:?}", self.text_line_buffer);

            // Look for complete lines ending with \n
            while let Some(newline_pos) = self.text_line_buffer.find('\n') {
                let line = self.text_line_buffer.drain(..=newline_pos).collect::<String>();
                let line = line.trim_end_matches('\n');
                eprintln!("MSGCHAN: processing line: {:?}", line);

                // Parse VSC:CCLLLL<base64> format
                if line.starts_with("VSC:") && line.len() >= 10 {
                    let channel_hex = &line[4..6];
                    let length_hex = &line[6..10];
                    let base64_data = &line[10..];
                    eprintln!("MSGCHAN: parsing VSC message - channel:{}, length:{}, data:{}", channel_hex, length_hex, base64_data);

                    if let (Ok(channel_id), Ok(expected_len)) = (
                        u8::from_str_radix(channel_hex, 16),
                        u16::from_str_radix(length_hex, 16),
                    ) {
                        match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64_data) {
                            Ok(payload_bytes) => {
                                eprintln!("MSGCHAN: decoded {} bytes, expected {}", payload_bytes.len(), expected_len);
                                if payload_bytes.len() == expected_len as usize {
                                    eprintln!("MSGCHAN: returning message successfully");
                                    return Ok(Message {
                                        channel_id: ChannelId(channel_id),
                                        payload: Bytes::from(payload_bytes),
                                    });
                                } else {
                                    eprintln!("MSGCHAN: Length mismatch: expected {}, got {}", expected_len, payload_bytes.len());
                                }
                            }
                            Err(e) => {
                                eprintln!("MSGCHAN: Base64 decode error: {}", e);
                            }
                        }
                    }
                } else {
                    eprintln!("MSGCHAN: Invalid line format: {}", line);
                }
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
                        // Add a timeout to the send operation
                        match tokio::time::timeout(std::time::Duration::from_secs(30), message_channel.send(message)).await {
                            Ok(result) => {
                                if let Err(e) = result {
                                    eprintln!("Error sending message: {}", e);
                                    break;
                                }
                            },
                            Err(_) => {
                                eprintln!("Timeout sending message");
                                break;
                            }
                        }
                    }

                    // Process incoming messages
                    result = message_channel.receive() => {
                        match result {
                            Ok(message) => {
                                // Store the channel_id before moving the message
                                let channel_id = message.channel_id;

                                // Add a timeout to the lock operation
                                let multiplexer_lock_result = tokio::time::timeout(
                                    std::time::Duration::from_secs(5), 
                                    multiplexer_clone.lock()
                                ).await;

                                match multiplexer_lock_result {
                                    Ok(mut multiplexer) => {
                                        if let Some(sender) = multiplexer.channels.get(&channel_id) {
                                            // Add a timeout to the send operation
                                            match tokio::time::timeout(
                                                std::time::Duration::from_secs(5),
                                                sender.send(message)
                                            ).await {
                                                Ok(send_result) => {
                                                    if send_result.is_err() {
                                                        // Channel was closed, remove it
                                                        multiplexer.channels.remove(&channel_id);
                                                    }
                                                },
                                                Err(_) => {
                                                    eprintln!("Timeout sending message to channel");
                                                    multiplexer.channels.remove(&channel_id);
                                                }
                                            }
                                        }
                                    },
                                    Err(_) => {
                                        eprintln!("Timeout acquiring multiplexer lock");
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
