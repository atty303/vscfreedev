mod shared;

use anyhow::Result;
use bytes::Bytes;
use shared::docker::RemoteContainer;
use std::time::Duration;
use tokio::time::sleep;
use vscfreedev_client::client;

#[tokio::test]
async fn test_basic_ssh_communication() -> Result<()> {
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Wait for SSH server to be ready
    sleep(Duration::from_secs(10)).await;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let mut message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    println!("Connected to remote host successfully");

    // Test basic message exchange
    let test_message = Bytes::from("Hello from client");
    println!("Sending test message: {:?}", test_message);

    message_channel.send(test_message.clone()).await?;
    println!("Message sent successfully");

    // Wait for response with timeout
    let response =
        tokio::time::timeout(Duration::from_secs(10), message_channel.receive()).await??;

    println!(
        "Received response: {:?}",
        String::from_utf8_lossy(&response)
    );

    // Verify it's an echo response
    let expected_response = format!("Echo: {}", String::from_utf8_lossy(&test_message));
    let actual_response = String::from_utf8_lossy(&response);
    if actual_response.contains("Echo:") {
        println!("Basic communication test passed!");
    } else {
        return Err(anyhow::anyhow!("Unexpected response: {}", actual_response));
    }

    Ok(())
}
