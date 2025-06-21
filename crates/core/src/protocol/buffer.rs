//! Protocol buffer utilities for batching and buffering messages

use super::simple::ResponseItem;
use std::collections::HashMap;

/// Generic protocol buffer for accumulating messages
pub trait ProtocolBuffer<T> {
    /// Add an item to the buffer
    fn add_item(&mut self, item: T);

    /// Check if the buffer has any data
    fn has_data(&self) -> bool;

    /// Take all items from the buffer
    fn take_items(&mut self) -> Vec<T>;

    /// Clear the buffer
    fn clear(&mut self);
}

/// Response buffer for the simple protocol
pub struct SimpleResponseBuffer {
    items: Vec<ResponseItem>,
    pending_connections: HashMap<u32, u16>,
    closed_connections: Vec<u32>,
}

impl SimpleResponseBuffer {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            pending_connections: HashMap::new(),
            closed_connections: Vec::new(),
        }
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

    pub fn add_port_forward_data(&mut self, connection_id: u32, data: bytes::Bytes) {
        self.add_item(ResponseItem::PortForwardData {
            connection_id,
            data,
        });
    }

    pub fn add_clipboard_content(&mut self, content: String) {
        self.add_item(ResponseItem::ClipboardContent { content });
    }

    pub fn is_connection_active(&self, connection_id: u32) -> bool {
        self.pending_connections.contains_key(&connection_id)
    }

    pub fn get_pending_connections(&self) -> &HashMap<u32, u16> {
        &self.pending_connections
    }

    pub fn get_closed_connections(&self) -> &[u32] {
        &self.closed_connections
    }
}

impl ProtocolBuffer<ResponseItem> for SimpleResponseBuffer {
    fn add_item(&mut self, item: ResponseItem) {
        self.items.push(item);
    }

    fn has_data(&self) -> bool {
        !self.items.is_empty()
    }

    fn take_items(&mut self) -> Vec<ResponseItem> {
        std::mem::take(&mut self.items)
    }

    fn clear(&mut self) {
        self.items.clear();
        self.pending_connections.clear();
        self.closed_connections.clear();
    }
}

impl Default for SimpleResponseBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Generic message buffer for any message type
#[derive(Debug, Clone)]
pub struct MessageBuffer<T> {
    items: Vec<T>,
    max_capacity: Option<usize>,
}

impl<T> MessageBuffer<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            max_capacity: None,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
            max_capacity: Some(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.max_capacity
            .map(|cap| self.items.len() >= cap)
            .unwrap_or(false)
    }
}

impl<T> ProtocolBuffer<T> for MessageBuffer<T> {
    fn add_item(&mut self, item: T) {
        if let Some(max_cap) = self.max_capacity {
            if self.items.len() >= max_cap {
                // Remove oldest item if at capacity
                self.items.remove(0);
            }
        }
        self.items.push(item);
    }

    fn has_data(&self) -> bool {
        !self.items.is_empty()
    }

    fn take_items(&mut self) -> Vec<T> {
        std::mem::take(&mut self.items)
    }

    fn clear(&mut self) {
        self.items.clear();
    }
}

impl<T> Default for MessageBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}
