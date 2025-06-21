//! Integration tests for IPC functionality

use anyhow::Result;
use tempfile::tempdir;
use tokio::time::{Duration, sleep};
use yuha_core::protocol::ResponseBuffer;
use yuha_remote::ipc::{IpcClient, IpcCommand, IpcResponse, IpcServer};

#[tokio::test]
async fn test_ipc_ping() -> Result<()> {
    let temp_dir = tempdir()?;
    let socket_path = temp_dir.path().join("test.sock");

    // Start IPC server
    let response_buffer = std::sync::Arc::new(tokio::sync::RwLock::new(ResponseBuffer::new()));
    let server = IpcServer::new(socket_path.clone(), response_buffer);

    tokio::spawn(async move {
        let _ = server.start().await;
    });

    // Wait for server to start
    sleep(Duration::from_millis(100)).await;

    // Test ping command
    let client = IpcClient::new(socket_path);
    let result = client.ping().await;

    assert!(result, "Ping should succeed");

    Ok(())
}

#[tokio::test]
async fn test_ipc_clipboard() -> Result<()> {
    let temp_dir = tempdir()?;
    let socket_path = temp_dir.path().join("test_clipboard.sock");

    // Start IPC server
    let response_buffer = std::sync::Arc::new(tokio::sync::RwLock::new(ResponseBuffer::new()));
    let server = IpcServer::new(socket_path.clone(), response_buffer);

    tokio::spawn(async move {
        let _ = server.start().await;
    });

    // Wait for server to start
    sleep(Duration::from_millis(100)).await;

    // Test clipboard operations
    let client = IpcClient::new(socket_path);

    // This test depends on clipboard availability, so we just test the IPC communication
    let response = client
        .send_command(IpcCommand::SetClipboard {
            content: "test content".to_string(),
        })
        .await?;

    // Should get a response (either success or error, depending on system)
    match response {
        IpcResponse::Success { .. } | IpcResponse::Error { .. } => {
            // Both are valid responses - depends on system clipboard availability
        }
        _ => panic!("Unexpected response type"),
    }

    Ok(())
}

#[tokio::test]
async fn test_ipc_status() -> Result<()> {
    let temp_dir = tempdir()?;
    let socket_path = temp_dir.path().join("test_status.sock");

    // Start IPC server
    let response_buffer = std::sync::Arc::new(tokio::sync::RwLock::new(ResponseBuffer::new()));
    let server = IpcServer::new(socket_path.clone(), response_buffer);

    tokio::spawn(async move {
        let _ = server.start().await;
    });

    // Wait for server to start
    sleep(Duration::from_millis(100)).await;

    // Test status command
    let client = IpcClient::new(socket_path);
    let (uptime, connected_clients, active_port_forwards) = client.status().await?;

    // uptime is u64, so always >= 0
    assert_eq!(
        connected_clients, 0,
        "No clients should be connected initially"
    );
    assert_eq!(
        active_port_forwards, 0,
        "No port forwards should be active initially"
    );

    Ok(())
}

#[tokio::test]
async fn test_ipc_send_to_client() -> Result<()> {
    let temp_dir = tempdir()?;
    let socket_path = temp_dir.path().join("test_send.sock");

    // Start IPC server
    let response_buffer = std::sync::Arc::new(tokio::sync::RwLock::new(ResponseBuffer::new()));
    let mut server = IpcServer::new(socket_path.clone(), response_buffer);

    // Set up client sender (simulate client connection)
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    server.set_client_sender(tx);

    tokio::spawn(async move {
        let _ = server.start().await;
    });

    // Wait for server to start
    sleep(Duration::from_millis(100)).await;

    // Test send to client
    let client = IpcClient::new(socket_path);
    let result = client.send_to_client("test message").await;

    assert!(result.is_ok(), "Send to client should succeed");

    // Check if message was received
    let received = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
    match received {
        Ok(Some(message)) => {
            assert_eq!(message, "test message");
        }
        _ => panic!("Message should be received"),
    }

    Ok(())
}

#[test]
fn test_ipc_command_serialization() {
    let cmd = IpcCommand::SetClipboard {
        content: "test".to_string(),
    };
    let json = serde_json::to_string(&cmd).unwrap();
    let parsed: IpcCommand = serde_json::from_str(&json).unwrap();

    match parsed {
        IpcCommand::SetClipboard { content } => {
            assert_eq!(content, "test");
        }
        _ => panic!("Wrong command type"),
    }
}

#[test]
fn test_ipc_response_serialization() {
    let resp = IpcResponse::Success {
        data: Some("test data".to_string()),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: IpcResponse = serde_json::from_str(&json).unwrap();

    match parsed {
        IpcResponse::Success { data } => {
            assert_eq!(data, Some("test data".to_string()));
        }
        _ => panic!("Wrong response type"),
    }
}
