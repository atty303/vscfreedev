//! Unified protocol abstraction for yuha
//!
//! This module provides a common abstraction for all protocol communications
//! in the yuha ecosystem, supporting both simple client-server and daemon protocols.

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Serialize, de::DeserializeOwned};
use std::fmt::Debug;

pub mod buffer;
pub mod codec;
pub mod daemon;
pub mod implementation;
pub mod simple;

use crate::error::Result;

// Re-export commonly used types
pub use implementation::{
    DaemonProtocol, DaemonProtocolFactory, GenericProtocol, SimpleProtocol, SimpleProtocolFactory,
};

/// Common trait for all protocol messages
pub trait Message: Serialize + DeserializeOwned + Debug + Clone + Send + Sync {}

/// Trait for protocol request types
pub trait Request: Message {}

/// Trait for protocol response types  
pub trait Response: Message {}

/// Unified protocol abstraction that can handle any request/response pair
#[async_trait]
pub trait Protocol<Req: Request, Resp: Response> {
    /// Send a request and receive a response
    async fn send_request(&mut self, request: Req) -> Result<Resp>;

    /// Receive a request (for server-side implementations)
    async fn receive_request(&mut self) -> Result<Req>;

    /// Send a response (for server-side implementations)
    async fn send_response(&mut self, response: Resp) -> Result<()>;
    /// Get the protocol name for debugging/logging
    fn name(&self) -> &'static str;
}

/// Protocol factory for creating protocol instances
pub trait ProtocolFactory<Req: Request, Resp: Response> {
    type Protocol: Protocol<Req, Resp>;
    type Config;
    fn create(config: Self::Config) -> Result<Self::Protocol>;
}

/// Message codec for encoding/decoding protocol messages
#[async_trait]
pub trait MessageCodec: Send + Sync {
    /// Encode a message to bytes
    fn encode<T: Message>(&self, message: &T) -> Result<Bytes>;

    /// Decode bytes to a message
    fn decode<T: Message>(&self, bytes: Bytes) -> Result<T>;
}
