mod shared;

use anyhow::Result;
use shared::docker::RemoteContainer;
use std::time::Duration;
use tokio::time::sleep;
use vscfreedev_client::client;

#[tokio::test]
async fn test_port_forward_start_only() -> Result<()> {
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Wait for SSH server to be ready
    sleep(Duration::from_secs(10)).await;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    println!("Connected to remote host successfully");

    // Create VscFreedevClient
    let client = vscfreedev_client::VscFreedevClient::new(message_channel);
    println!("VscFreedevClient created successfully");

    let local_port = 9999;
    let remote_port = 8888;

    // Try to start port forwarding with extended timeout
    println!(
        "Starting port forward: {} -> localhost:{}",
        local_port, remote_port
    );

    let start_result = tokio::time::timeout(
        Duration::from_secs(60),
        client.start_port_forward(local_port, "localhost", remote_port),
    )
    .await;
    match start_result {
        Ok(Ok(())) => {
            println!("Port forwarding started successfully!");
            // Clean up by stopping the port forward
            let _ = tokio::time::timeout(
                Duration::from_secs(30),
                client.stop_port_forward(local_port),
            )
            .await;
        }
        Ok(Err(e)) => {
            println!("Port forwarding failed with error: {}", e);
            return Err(anyhow::anyhow!("Port forwarding failed: {}", e));
        }
        Err(_) => {
            println!("Port forwarding timed out!");
            return Err(anyhow::anyhow!(
                "Port forwarding timed out after 30 seconds"
            ));
        }
    }

    Ok(())
}
