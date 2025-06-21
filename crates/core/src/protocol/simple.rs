//! # Yuha Protocol Implementation
//!
//! This module defines the protocol used for direct client-server communication.
//! It follows a straightforward request-response pattern with long polling support
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

use super::Message;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

impl Message for ProtocolRequest {}
impl super::Request for ProtocolRequest {}

impl Message for ProtocolResponse {}
impl super::Response for ProtocolResponse {}

impl Message for ResponseItem {}

/// Response buffer for accumulating response items
pub struct ResponseBuffer {
    items: Vec<ResponseItem>,
    pending_connections: HashMap<u32, u16>,
    closed_connections: Vec<u32>,
}

impl ResponseBuffer {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            pending_connections: HashMap::new(),
            closed_connections: Vec::new(),
        }
    }

    pub fn add_item(&mut self, item: ResponseItem) {
        self.items.push(item);
    }

    pub fn add_new_connection(&mut self, connection_id: u32, local_port: u16) {
        self.pending_connections.insert(connection_id, local_port);
        self.add_item(ResponseItem::NewConnection {
            connection_id,
            local_port,
        });
    }

    pub fn add_close_connection(&mut self, connection_id: u32) {
        self.pending_connections.remove(&connection_id);
        self.closed_connections.push(connection_id);
        self.add_item(ResponseItem::CloseConnection { connection_id });
    }

    pub fn add_port_forward_data(&mut self, connection_id: u32, data: Bytes) {
        self.add_item(ResponseItem::PortForwardData {
            connection_id,
            data,
        });
    }

    pub fn add_clipboard_content(&mut self, content: String) {
        self.add_item(ResponseItem::ClipboardContent { content });
    }

    pub fn has_data(&self) -> bool {
        !self.items.is_empty()
    }

    pub fn take_items(&mut self) -> Vec<ResponseItem> {
        std::mem::take(&mut self.items)
    }

    pub fn is_connection_active(&self, connection_id: u32) -> bool {
        self.pending_connections.contains_key(&connection_id)
    }
}

impl Default for ResponseBuffer {
    fn default() -> Self {
        Self::new()
    }
}
