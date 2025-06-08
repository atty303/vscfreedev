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

/// A bidirectional message channel for binary communication
///
/// Wire format:
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
    /// Create a new message channel from any stream that implements AsyncRead + AsyncWrite + Unpin
    pub fn new_with_stream(stream: T) -> Self {
        Self {
            inner: stream,
            read_buffer: BytesMut::with_capacity(4096),
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

        // Write payload length (2 bytes, big endian)
        self.inner.write_u16(payload_len as u16).await?;

        // Write payload
        self.inner.write_all(&payload).await?;

        // Explicitly flush the stream to ensure data is sent
        self.inner.flush().await?;

        Ok(())
    }

    /// Receive a message from the channel
    pub async fn receive(&mut self) -> Result<Bytes> {
        self.receive_binary().await
    }

    async fn receive_binary(&mut self) -> Result<Bytes> {
        loop {
            // Try to read a complete message from the buffer
            if self.read_buffer.len() >= 2 {
                // Read payload length without advancing the cursor
                let payload_len =
                    u16::from_be_bytes([self.read_buffer[0], self.read_buffer[1]]) as usize;

                // Check if we have the complete message
                if self.read_buffer.len() >= 2 + payload_len {
                    // Advance the cursor past the header
                    self.read_buffer.advance(2);

                    // Extract the payload
                    let payload = self.read_buffer.split_to(payload_len).freeze();

                    return Ok(payload);
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
