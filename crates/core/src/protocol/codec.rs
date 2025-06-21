//! Message codecs for different serialization formats

use super::{Message, MessageCodec};
use crate::error::Result;
use bytes::Bytes;

/// JSON codec implementation
#[derive(Debug, Clone, Default)]
pub struct JsonCodec;

impl JsonCodec {
    pub fn new() -> Self {
        Self
    }
}

impl MessageCodec for JsonCodec {
    fn encode<T: Message>(&self, message: &T) -> Result<Bytes> {
        let json_data = serde_json::to_vec(message).map_err(|e| {
            crate::error::YuhaError::protocol(format!("JSON encoding failed: {}", e))
        })?;
        Ok(Bytes::from(json_data))
    }
    fn decode<T: Message>(&self, bytes: Bytes) -> Result<T> {
        serde_json::from_slice(&bytes)
            .map_err(|e| crate::error::YuhaError::protocol(format!("JSON decoding failed: {}", e)))
    }
}

/// Binary codec for more efficient serialization (future implementation)
#[derive(Debug, Clone, Default)]
pub struct BinaryCodec;

impl BinaryCodec {
    pub fn new() -> Self {
        Self
    }
}

impl MessageCodec for BinaryCodec {
    fn encode<T: Message>(&self, _message: &T) -> Result<Bytes> {
        // TODO: Implement binary encoding using bincode or similar
        todo!("Binary codec not yet implemented")
    }
    fn decode<T: Message>(&self, _bytes: Bytes) -> Result<T> {
        // TODO: Implement binary decoding using bincode or similar
        todo!("Binary codec not yet implemented")
    }
}

/// Compressed JSON codec for large messages
#[derive(Debug, Clone, Default)]
pub struct CompressedJsonCodec {
    compression_level: u32,
}

impl CompressedJsonCodec {
    pub fn new(compression_level: u32) -> Self {
        Self { compression_level }
    }
}

impl MessageCodec for CompressedJsonCodec {
    fn encode<T: Message>(&self, _message: &T) -> Result<Bytes> {
        // TODO: Implement compressed JSON encoding
        todo!("Compressed JSON codec not yet implemented")
    }
    fn decode<T: Message>(&self, _bytes: Bytes) -> Result<T> {
        // TODO: Implement compressed JSON decoding
        todo!("Compressed JSON codec not yet implemented")
    }
}
