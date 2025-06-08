use bytes::{Buf, Bytes, BytesMut};
use std::io;
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

/// A simple message for direct client-remote communication
#[derive(Debug, Clone)]
pub struct Message {
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
/// - 2 bytes: payload length (big endian)
/// - N bytes: payload
///
/// Text-safe mode wire format:
/// - "VSC:" prefix
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
    pub async fn send(&mut self, payload: Bytes) -> Result<()> {
        let payload_len = payload.len();

        if payload_len > u16::MAX as usize {
            return Err(ChannelError::Protocol(format!(
                "Payload too large: {}",
                payload_len
            )));
        }

        match self.mode {
            TransportMode::Binary => {
                eprintln!("MSGCHAN_BINARY: sending binary message, payload_len: {}", payload_len);
                eprintln!("MSGCHAN_BINARY: payload bytes: {:?}", &payload[..std::cmp::min(10, payload.len())]);
                
                // Write payload length (2 bytes, big endian)
                let length_bytes = (payload_len as u16).to_be_bytes();
                eprintln!("MSGCHAN_BINARY: sending length header: {:?}", length_bytes);
                self.inner.write_u16(payload_len as u16).await?;

                // Write payload
                eprintln!("MSGCHAN_BINARY: sending payload of {} bytes", payload.len());
                self.inner.write_all(&payload).await?;
            }
            TransportMode::TextSafe => {
                // Encode as text-safe format: VSC:LLLL<base64>\n
                let encoded_payload = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &payload);
                let frame = format!("VSC:{:04X}{}\n", payload_len, encoded_payload);
                self.inner.write_all(frame.as_bytes()).await?;
            }
        }

        // Explicitly flush the stream to ensure data is sent
        self.inner.flush().await?;

        Ok(())
    }

    /// Receive a message from the channel
    pub async fn receive(&mut self) -> Result<Bytes> {
        match self.mode {
            TransportMode::Binary => self.receive_binary().await,
            TransportMode::TextSafe => self.receive_text_safe().await,
        }
    }

    async fn receive_binary(&mut self) -> Result<Bytes> {
        eprintln!("MSGCHAN_BINARY: receive_binary starting");
        loop {
            eprintln!("MSGCHAN_BINARY: buffer length: {}", self.read_buffer.len());
            if self.read_buffer.len() > 0 {
                eprintln!("MSGCHAN_BINARY: buffer contents: {:?}", &self.read_buffer[..std::cmp::min(10, self.read_buffer.len())]);
            }
            
            // Try to read a complete message from the buffer
            if self.read_buffer.len() >= 2 {
                // Read payload length without advancing the cursor
                let payload_len =
                    u16::from_be_bytes([self.read_buffer[0], self.read_buffer[1]]) as usize;
                eprintln!("MSGCHAN_BINARY: parsed payload length: {}", payload_len);

                // Check if we have the complete message
                if self.read_buffer.len() >= 2 + payload_len {
                    eprintln!("MSGCHAN_BINARY: complete message available, extracting payload");
                    // Advance the cursor past the header
                    self.read_buffer.advance(2);

                    // Extract the payload
                    let payload = self.read_buffer.split_to(payload_len).freeze();
                    eprintln!("MSGCHAN_BINARY: returning payload of {} bytes", payload.len());
                    return Ok(payload);
                } else {
                    eprintln!("MSGCHAN_BINARY: incomplete message, need {} more bytes", (2 + payload_len) - self.read_buffer.len());
                }
            } else {
                eprintln!("MSGCHAN_BINARY: not enough bytes for header");
            }

            // Read more data into the buffer
            eprintln!("MSGCHAN_BINARY: reading more data from stream");
            let bytes_read = self.inner.read_buf(&mut self.read_buffer).await?;
            eprintln!("MSGCHAN_BINARY: read {} bytes from stream", bytes_read);

            if bytes_read == 0 {
                eprintln!("MSGCHAN_BINARY: EOF on stream");
                return Err(ChannelError::Closed);
            }
        }
    }

    async fn receive_text_safe(&mut self) -> Result<Bytes> {
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

                // Parse VSC:LLLL<base64> format
                if line.starts_with("VSC:") && line.len() >= 8 {
                    let length_hex = &line[4..8];
                    let base64_data = &line[8..];
                    eprintln!("MSGCHAN: parsing VSC message - length:{}, data:{}", length_hex, base64_data);

                    if let Ok(expected_len) = u16::from_str_radix(length_hex, 16) {
                        match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64_data) {
                            Ok(payload_bytes) => {
                                eprintln!("MSGCHAN: decoded {} bytes, expected {}", payload_bytes.len(), expected_len);
                                if payload_bytes.len() == expected_len as usize {
                                    eprintln!("MSGCHAN: returning message successfully");
                                    return Ok(Bytes::from(payload_bytes));
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn test_send_receive_binary() {
        let (client, server) = duplex(1024);

        let mut client_channel = MessageChannel::new_with_stream(client);
        let mut server_channel = MessageChannel::new_with_stream(server);

        let test_data = Bytes::from_static(b"Hello, server!");

        tokio::spawn(async move {
            client_channel.send(test_data).await.unwrap();
        });

        // Server receives the message
        let received = server_channel.receive().await.unwrap();
        assert_eq!(received, Bytes::from_static(b"Hello, server!"));
    }
}
