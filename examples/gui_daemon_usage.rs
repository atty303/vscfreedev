//! Example of how a GUI application can use the daemon functionality
//! This demonstrates how the GUI crate can use the same daemon as the CLI

use anyhow::Result;
use yuha_client::{daemon, daemon_client::DaemonClient};
use yuha_core::transport::{TransportConfig, TransportType, SshTransportConfig};
use std::path::PathBuf;

/// Example GUI daemon client
pub struct GuiDaemonClient {
    daemon_client: DaemonClient,
}

impl GuiDaemonClient {
    /// Connect to the daemon (same as CLI)
    pub async fn connect() -> Result<Self> {
        let daemon_client = DaemonClient::connect(None).await?;
        Ok(Self { daemon_client })
    }

    /// Check if daemon is running
    pub async fn is_daemon_running(&mut self) -> bool {
        self.daemon_client.ping().await.unwrap_or(false)
    }

    /// Start daemon if not running
    pub async fn ensure_daemon_running() -> Result<()> {
        // For GUI, we might want to start daemon in background automatically
        daemon::run_daemon(
            None,        // config
            None,        // socket  
            false,       // foreground (run in background)
            None,        // log_file
            None,        // pid_file
            false,       // verbose
        ).await
    }

    /// Create a new SSH session for GUI
    pub async fn create_ssh_session(
        &mut self,
        name: String,
        host: String,
        port: u16,
        username: String,
        password: Option<String>,
        key_path: Option<PathBuf>,
    ) -> Result<yuha_core::session::SessionId> {
        let transport_config = TransportConfig {
            transport_type: TransportType::Ssh,
            ssh: Some(SshTransportConfig {
                host,
                port,
                username,
                password,
                key_path,
                auto_upload_binary: false,
                timeout: 30,
                keepalive: 60,
            }),
            local: None,
            tcp: None,
            wsl: None,
            general: Default::default(),
        };

        let (session_id, _reused) = self
            .daemon_client
            .create_session(name, transport_config, vec![], None)
            .await?;

        Ok(session_id)
    }

    /// List all sessions for GUI display
    pub async fn list_sessions(&mut self) -> Result<Vec<yuha_client::daemon_protocol::SessionSummary>> {
        self.daemon_client.list_sessions().await
    }

    /// Execute clipboard operation
    pub async fn set_clipboard(&mut self, session_id: yuha_core::session::SessionId, content: String) -> Result<()> {
        use yuha_client::daemon_protocol::DaemonCommand;
        
        self.daemon_client
            .execute_command(session_id, DaemonCommand::SetClipboard { content })
            .await?;
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("GUI Daemon Usage Example");
    
    // Check if daemon is running
    match GuiDaemonClient::connect().await {
        Ok(mut client) => {
            if client.is_daemon_running().await {
                println!("✓ Daemon is running");
                
                // List sessions
                let sessions = client.list_sessions().await?;
                println!("Active sessions: {}", sessions.len());
                
                for session in sessions {
                    println!("  - {} ({})", session.name, session.status);
                }
            } else {
                println!("✗ Daemon is not responding");
            }
        }
        Err(_) => {
            println!("✗ Daemon is not running");
            println!("GUI could start daemon automatically here");
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gui_daemon_connect() {
        // This test shows how GUI can attempt to connect to daemon
        let result = GuiDaemonClient::connect().await;
        // In real usage, GUI would handle the error by starting daemon
        match result {
            Ok(_) => println!("GUI connected to daemon"),
            Err(_) => println!("GUI needs to start daemon"),
        }
    }
}