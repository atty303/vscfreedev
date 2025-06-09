mod shared;

use anyhow::Result;
use bytes::Bytes;
use shared::docker::RemoteContainer;
use std::time::Duration;
use tokio::time::sleep;
use vscfreedev_client::client;

#[tokio::test]
async fn test_vscfreedev_client_basic() -> Result<()> {
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

    // Test basic message exchange through the client
    let test_message = Bytes::from("Hello from VscFreedevClient");
    println!("Sending test message: {:?}", test_message);

    client.send_message(test_message.clone()).await?;
    println!("Message sent successfully");

    // Wait for response with timeout
    let response =
        tokio::time::timeout(Duration::from_secs(10), client.receive_message()).await??;

    println!(
        "Received response: {:?}",
        String::from_utf8_lossy(&response)
    );

    let actual_response = String::from_utf8_lossy(&response);
    if actual_response.contains("Echo:") {
        println!("VscFreedevClient basic communication test passed!");
    } else {
        return Err(anyhow::anyhow!("Unexpected response: {}", actual_response));
    }

    Ok(())
}
