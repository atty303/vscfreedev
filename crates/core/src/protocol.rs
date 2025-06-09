use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum YuhaRequest {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum YuhaResponse {
    Data { items: Vec<ResponseItem> },
    Success,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseItem {
    PortForwardData { connection_id: u32, data: Bytes },
    NewConnection { connection_id: u32, local_port: u16 },
    CloseConnection { connection_id: u32 },
    ClipboardContent { content: String },
}

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
