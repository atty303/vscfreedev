//! # Protocol Layer
//!
//! This module provides a unified abstraction for all protocol communications
//! in the Yuha ecosystem. It supports both simple client-server and daemon protocols
//! while maintaining a consistent interface.
//!
//! ## Protocol Types
//!
//! - **Protocol**: Direct request-response communication between client and remote server
//! - **Daemon Protocol**: Communication with local daemon for managing multiple sessions
//!
//! ## Design Philosophy
//!
//! The protocol layer follows a request-response pattern with long polling
//! for pseudo-bidirectional communication. This approach prioritizes:
//!
//! - **Simplicity**: Easy to understand and debug
//! - **Reliability**: Predictable error handling and state management
//! - **Testability**: Straightforward to unit test and mock
//!
//! ## Message Flow
//!
//! ```text
//! Client → Server: Request (JSON over binary framing)
//! Server → Client: Response (JSON over binary framing)
//!
//! For bidirectional data:
//! Client → Server: PollData request (long polling)
//! Server → Client: Response with buffered data items
//! ```
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use yuha_core::protocol::{Protocol, ProtocolRequest};
//!
//! // Send a request through any protocol implementation
//! let response = protocol.send_request(ProtocolRequest::GetClipboard).await?;
//! ```

pub mod buffer;
pub mod daemon;
pub mod request_response;

// Re-export main protocol types for convenient access
pub use buffer::ResponseBuffer;
pub use request_response::{ProtocolRequest, ProtocolResponse, ResponseItem};
