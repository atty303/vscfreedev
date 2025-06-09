mod shared;

use anyhow::Result;
use bytes::Bytes;
use shared::docker::RemoteContainer;
use std::time::Duration;
use yuha_client::client;
use yuha_core::port_forward::PortForwardMessage;

#[tokio::test]
async fn test_direct_port_forward_request() -> Result<()> {
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let mut message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    println!("Connected to remote host successfully");

    // Use port 0 to let the system choose an available port
    let local_port = 0; // Let system choose
    let remote_port = 8888; // Echo service port in the container

    // Create and send port forward request directly
    let request = PortForwardMessage::StartRequest {
        local_port,
        remote_host: "localhost".to_string(),
        remote_port,
    };

    let request_json = serde_json::to_string(&request)?;
    println!("Sending port forward request: {}", request_json);

    message_channel.send(Bytes::from(request_json)).await?;
    println!("Request sent successfully");

    // Wait for response with extended timeout
    let response_bytes =
        tokio::time::timeout(Duration::from_secs(30), message_channel.receive()).await??;

    let response_str = String::from_utf8_lossy(&response_bytes);
    println!("Received response: {}", response_str);

    // Try to parse as port forward response
    match serde_json::from_str::<PortForwardMessage>(&response_str) {
        Ok(PortForwardMessage::StartResponse { success, error, .. }) => {
            println!(
                "Parsed StartResponse: success={}, error={:?}",
                success, error
            );
            if success {
                println!("Port forward started successfully!");
            } else {
                println!("Port forward failed: {:?}", error);
            }
        }
        Ok(other) => {
            println!("Received unexpected port forward message: {:?}", other);
        }
        Err(e) => {
            println!("Failed to parse response as port forward message: {}", e);
            println!("Raw response was: {}", response_str);
        }
    }

    Ok(())
}
