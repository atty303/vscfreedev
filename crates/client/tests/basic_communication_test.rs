mod shared;

use anyhow::Result;
use bytes::Bytes;
use shared::docker::RemoteContainer;
use std::time::Duration;
use yuha_client::client;

#[tokio::test]
async fn test_basic_ssh_communication() -> Result<()> {
    let test_start = std::time::Instant::now();
    println!("Basic test started at: {:?}", test_start);

    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let container_ready = std::time::Instant::now();
    println!(
        "Basic test - Docker container ready in: {:?}",
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
        "Basic test - SSH connection completed in: {:?}",
        ssh_connected.duration_since(ssh_connect_start)
    );
    println!(
        "Basic test - Total time to SSH ready: {:?}",
        ssh_connected.duration_since(test_start)
    );

    // Test basic message exchange
    let test_message = Bytes::from("Hello from client");
    println!("Sending test message: {:?}", test_message);

    let message_send_start = std::time::Instant::now();
    message_channel.send(test_message.clone()).await?;
    println!("Message sent successfully");

    // Wait for response with timeout
    let response =
        tokio::time::timeout(Duration::from_secs(5), message_channel.receive()).await??;

    let response_received = std::time::Instant::now();
    println!(
        "Basic test - Echo response received in: {:?}",
        response_received.duration_since(message_send_start)
    );
    println!(
        "Basic test - Total test time: {:?}",
        response_received.duration_since(test_start)
    );

    println!(
        "Received response: {:?}",
        String::from_utf8_lossy(&response)
    );

    // Verify it's an echo response
    let _expected_response = format!("Echo: {}", String::from_utf8_lossy(&test_message));
    let actual_response = String::from_utf8_lossy(&response);
    if actual_response.contains("Echo:") {
        println!("Basic communication test passed!");
    } else {
        return Err(anyhow::anyhow!("Unexpected response: {}", actual_response));
    }

    Ok(())
}
