use bytes::{Buf, Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, warn};

use crate::error::{ProtocolError as ChannelError, Result};
use crate::protocol::simple::{YuhaRequest, YuhaResponse};

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

    /// Send a request over the channel
    pub async fn send_request(&mut self, request: &YuhaRequest) -> Result<()> {
        debug!("Sending request: {:?}", request);

        let json_data = serde_json::to_vec(request).map_err(|e| {
            warn!("Failed to serialize request: {}", e);
            ChannelError::serialization(format!("Request serialization failed: {}", e))
        })?;

        self.send(Bytes::from(json_data)).await?;
        debug!("Request sent successfully");
        Ok(())
    }

    /// Send a response over the channel
    pub async fn send_response(&mut self, response: &YuhaResponse) -> Result<()> {
        debug!("Sending response: {:?}", response);

        let json_data = serde_json::to_vec(response).map_err(|e| {
            warn!("Failed to serialize response: {}", e);
            ChannelError::serialization(format!("Response serialization failed: {}", e))
        })?;

        self.send(Bytes::from(json_data)).await?;
        debug!("Response sent successfully");
        Ok(())
    }

    /// Receive a request from the channel
    pub async fn receive_request(&mut self) -> Result<YuhaRequest> {
        debug!("Waiting for request...");

        let payload = self.receive().await?;
        debug!("Received request payload ({} bytes)", payload.len());

        let result = serde_json::from_slice(&payload).map_err(|e| {
            warn!("Failed to deserialize request: {}", e);
            ChannelError::serialization(format!("Request deserialization failed: {}", e))
        })?;

        debug!("Successfully parsed request: {:?}", result);
        Ok(result)
    }

    /// Receive a response from the channel
    pub async fn receive_response(&mut self) -> Result<YuhaResponse> {
        debug!("Waiting for response...");

        let payload = self.receive().await?;
        debug!("Received response payload ({} bytes)", payload.len());

        let result = serde_json::from_slice(&payload).map_err(|e| {
            warn!("Failed to deserialize response: {}", e);
            ChannelError::serialization(format!("Response deserialization failed: {}", e))
        })?;

        debug!("Successfully parsed response: {:?}", result);
        Ok(result)
    }

    /// Send a raw message over the channel
    pub async fn send(&mut self, payload: Bytes) -> Result<()> {
        let payload_len = payload.len();
        debug!("Sending message of {} bytes", payload_len);

        if payload_len > u16::MAX as usize {
            warn!(
                "Payload too large: {} bytes (max: {})",
                payload_len,
                u16::MAX
            );
            return Err(ChannelError::buffer_overflow(payload_len).into());
        }

        // Write payload length (2 bytes, big endian)
        self.inner
            .write_u16(payload_len as u16)
            .await
            .map_err(|e| {
                warn!("Failed to write payload length: {}", e);
                e
            })?;

        // Write payload
        self.inner.write_all(&payload).await.map_err(|e| {
            warn!("Failed to write payload: {}", e);
            e
        })?;

        // Explicitly flush the stream to ensure data is sent
        self.inner.flush().await.map_err(|e| {
            warn!("Failed to flush stream: {}", e);
            e
        })?;

        debug!("Message sent successfully");
        Ok(())
    }

    /// Receive a message from the channel
    pub async fn receive(&mut self) -> Result<Bytes> {
        // No timeout - block until data is available
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
                return Err(ChannelError::Closed.into());
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
