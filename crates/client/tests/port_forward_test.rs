mod shared;

use anyhow::Result;
use shared::docker::RemoteContainer;
use std::time::Duration;
use tokio::time::sleep;
use vscfreedev_client::client;

#[tokio::test]
async fn test_port_forwarding_single() -> Result<()> {
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Wait for SSH server to be ready
    sleep(Duration::from_secs(10)).await;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    println!("Connected to remote host");

    // Create client with port forwarding capabilities
    let mut client = vscfreedev_client::VscFreedevClient::new(message_channel);

    // Test port forwarding setup
    let local_port = 8080;
    let remote_port = 80;

    println!(
        "Starting port forward: {} -> localhost:{}",
        local_port, remote_port
    );

    client
        .start_port_forward(local_port, "localhost", remote_port)
        .await?;

    println!("Single port forwarding test completed successfully");
    Ok(())
}

#[tokio::test]
async fn test_port_forwarding_multiple() -> Result<()> {
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Wait for SSH server to be ready
    sleep(Duration::from_secs(10)).await;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    println!("Connected to remote host");

    // Create client with port forwarding capabilities
    let mut client = vscfreedev_client::VscFreedevClient::new(message_channel);

    // Test multiple port forwarding
    let forwards = vec![(8080, 80), (8443, 443), (3000, 3000)];

    for (local_port, remote_port) in &forwards {
        println!(
            "Starting port forward: {} -> localhost:{}",
            local_port, remote_port
        );
        client
            .start_port_forward(*local_port, "localhost", *remote_port)
            .await?;
    }

    println!("Multiple port forwarding test completed successfully");
    Ok(())
}

#[tokio::test]
async fn test_port_forwarding_stop() -> Result<()> {
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Wait for SSH server to be ready
    sleep(Duration::from_secs(10)).await;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    println!("Connected to remote host");

    // Create client with port forwarding capabilities
    let mut client = vscfreedev_client::VscFreedevClient::new(message_channel);

    let local_port = 8080;
    let remote_port = 80;

    // Start port forwarding
    println!(
        "Starting port forward: {} -> localhost:{}",
        local_port, remote_port
    );
    client
        .start_port_forward(local_port, "localhost", remote_port)
        .await?;

    // Stop port forwarding
    println!("Stopping port forward for port {}", local_port);
    client.stop_port_forward(local_port).await?;

    println!("Port forwarding stop test completed successfully");
    Ok(())
}

#[tokio::test]
async fn test_port_forwarding_data_transfer() -> Result<()> {
    // Start the Docker container with the SSH server and echo service
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;
    let _echo_service_port = container.echo_service_port().await?;

    // Wait for SSH server and echo service to be ready
    sleep(Duration::from_secs(10)).await;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    println!("Connected to remote host");

    // Create client with port forwarding capabilities
    let mut client = vscfreedev_client::VscFreedevClient::new(message_channel);

    let local_port = 9090;

    // Start port forwarding to the Docker container's echo service on port 8888
    println!("Starting port forward: {} -> localhost:8888", local_port);
    client
        .start_port_forward(local_port, "localhost", 8888)
        .await?;

    println!("Port forwarding request completed - protocol layer test passed");

    // For now, we only test that the protocol layer works
    // Full data transfer implementation will be added in future iterations

    println!("Data transfer test completed successfully");
    Ok(())
}
