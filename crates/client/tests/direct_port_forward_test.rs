mod shared;

use anyhow::Result;
use bytes::Bytes;
use shared::docker::RemoteContainer;
use std::time::Duration;
use yuha_client::client;
use yuha_core::port_forward::PortForwardMessage;

#[tokio::test]
async fn test_direct_port_forward_request() -> Result<()> {
    let test_start = std::time::Instant::now();
    println!("Test started at: {:?}", test_start);

    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let container_ready = std::time::Instant::now();
    println!(
        "Docker container ready in: {:?}",
        container_ready.duration_since(test_start)
    );

    let ssh_port = container.ssh_port().await?;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let ssh_connect_start = std::time::Instant::now();
    let mut message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    let ssh_connected = std::time::Instant::now();
    println!(
        "SSH connection completed in: {:?}",
        ssh_connected.duration_since(ssh_connect_start)
    );
    println!(
        "Total time to SSH ready: {:?}",
        ssh_connected.duration_since(test_start)
    );

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

    let request_send_start = std::time::Instant::now();
    message_channel.send(Bytes::from(request_json)).await?;
    println!("Request sent successfully");

    // Wait for response with timeout
    let response_bytes =
        tokio::time::timeout(Duration::from_secs(15), message_channel.receive()).await??;

    let response_received = std::time::Instant::now();
    println!(
        "Port forward response received in: {:?}",
        response_received.duration_since(request_send_start)
    );
    println!(
        "Total test time: {:?}",
        response_received.duration_since(test_start)
    );

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
