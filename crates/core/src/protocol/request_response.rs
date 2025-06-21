//! # Protocol Implementation
//!
//! This module defines the protocol used for direct client-server communication.
//! It follows a request-response pattern with long polling support
//! for pseudo-bidirectional communication.
//!
//! ## Request Types
//!
//! The protocol supports the following request types:
//!
//! - **PollData**: Long polling for receiving server-side data
//! - **Port Forwarding**: Start/stop port forwarding and data transfer
//! - **Clipboard Operations**: Get/set clipboard content
//! - **Browser Operations**: Open URLs in the default browser
//!
//! ## Response Format
//!
//! All responses follow a consistent format:
//!
//! - **Success**: Operation completed successfully
//! - **Error**: Operation failed with error message
//! - **Data**: Contains multiple data items from polling
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use yuha_core::protocol::{ProtocolRequest, ProtocolResponse};
//!
//! // Create a clipboard request
//! let request = ProtocolRequest::GetClipboard;
//!
//! // Send via protocol implementation
//! let response = protocol.send_request(request).await?;
//!
//! match response {
//!     ProtocolResponse::Data { items } => {
//!         // Process clipboard data
//!     }
//!     ProtocolResponse::Error { message } => {
//!         // Handle error
//!     }
//!     _ => {}
//! }
//! ```

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Protocol request types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolRequest {
    PollData,
    StartPortForward {
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    },
    StopPortForward {
        local_port: u16,
    },
    PortForwardData {
        connection_id: u32,
        data: Bytes,
    },
    GetClipboard,
    SetClipboard {
        content: String,
    },
    OpenBrowser {
        url: String,
    },
}

/// Protocol response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolResponse {
    Data { items: Vec<ResponseItem> },
    Success,
    Error { message: String },
}

/// Response data items for the simple protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseItem {
    PortForwardData { connection_id: u32, data: Bytes },
    NewConnection { connection_id: u32, local_port: u16 },
    CloseConnection { connection_id: u32 },
    ClipboardContent { content: String },
}
