use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tracing::info;

/// Port forwarding protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PortForwardMessage {
    /// Request to start port forwarding
    StartRequest {
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    },
    /// Response to start request
    StartResponse {
        local_port: u16,
        success: bool,
        error: Option<String>,
    },
    /// Request to stop port forwarding
    StopRequest { local_port: u16 },
    /// Response to stop request
    StopResponse {
        local_port: u16,
        success: bool,
        error: Option<String>,
    },
    /// Data transfer for a specific forwarded connection
    Data {
        local_port: u16,
        connection_id: u32,
        data: Vec<u8>,
    },
    /// New connection established
    NewConnection { local_port: u16, connection_id: u32 },
    /// Connection closed
    CloseConnection { local_port: u16, connection_id: u32 },
}

/// A single port forwarding instance
#[derive(Debug)]
pub struct PortForward {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub listener: TcpListener,
    pub connections: Arc<RwLock<HashMap<u32, mpsc::UnboundedSender<Bytes>>>>,
    pub next_connection_id: Arc<RwLock<u32>>,
}

impl PortForward {
    /// Create a new port forward
    pub async fn new(
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    ) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], local_port))).await?;

        Ok(Self {
            local_port,
            remote_host,
            remote_port,
            listener,
            connections: Arc::new(RwLock::new(HashMap::new())),
            next_connection_id: Arc::new(RwLock::new(1)),
        })
    }

    /// Get the next connection ID
    pub async fn next_connection_id(&self) -> u32 {
        let mut id = self.next_connection_id.write().await;
        let current = *id;
        *id += 1;
        current
    }

    /// Add a connection
    pub async fn add_connection(&self, connection_id: u32, sender: mpsc::UnboundedSender<Bytes>) {
        self.connections.write().await.insert(connection_id, sender);
    }

    /// Remove a connection
    pub async fn remove_connection(&self, connection_id: u32) {
        self.connections.write().await.remove(&connection_id);
    }

    /// Send data to a specific connection
    pub async fn send_to_connection(&self, connection_id: u32, data: Bytes) -> anyhow::Result<()> {
        let connections = self.connections.read().await;
        if let Some(sender) = connections.get(&connection_id) {
            sender.send(data)?;
        }
        Ok(())
    }
}

/// Port forwarding manager
#[derive(Debug)]
pub struct PortForwardManager {
    forwards: Arc<RwLock<HashMap<u16, Arc<PortForward>>>>,
}

impl PortForwardManager {
    /// Create a new port forward manager
    pub fn new() -> Self {
        Self {
            forwards: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a new port forward
    pub async fn start_forward(
        &self,
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    ) -> anyhow::Result<()> {
        // Check if port is already forwarded
        {
            let forwards = self.forwards.read().await;
            if forwards.contains_key(&local_port) {
                return Err(anyhow::anyhow!(
                    "Port {} is already being forwarded",
                    local_port
                ));
            }
        }

        // Create new port forward
        let port_forward = Arc::new(PortForward::new(local_port, remote_host, remote_port).await?);

        // Store it
        {
            let mut forwards = self.forwards.write().await;
            forwards.insert(local_port, port_forward.clone());
        }

        info!(
            "Started port forwarding: {} -> {}:{}",
            local_port, port_forward.remote_host, port_forward.remote_port
        );
        Ok(())
    }

    /// Stop a port forward
    pub async fn stop_forward(&self, local_port: u16) -> anyhow::Result<()> {
        let mut forwards = self.forwards.write().await;
        if forwards.remove(&local_port).is_some() {
            info!("Stopped port forwarding for port {}", local_port);
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "No port forwarding found for port {}",
                local_port
            ))
        }
    }

    /// Get a port forward by local port
    pub async fn get_forward(&self, local_port: u16) -> Option<Arc<PortForward>> {
        let forwards = self.forwards.read().await;
        forwards.get(&local_port).cloned()
    }

    /// List all active forwards
    pub async fn list_forwards(&self) -> Vec<(u16, String, u16)> {
        let forwards = self.forwards.read().await;
        forwards
            .values()
            .map(|f| (f.local_port, f.remote_host.clone(), f.remote_port))
            .collect()
    }
}

impl Default for PortForwardManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Legacy start function for compatibility
pub async fn start() -> anyhow::Result<()> {
    Ok(())
}
