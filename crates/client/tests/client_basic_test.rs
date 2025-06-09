mod shared;

use anyhow::Result;
use bytes::Bytes;
use shared::docker::RemoteContainer;
use std::time::Duration;
use yuha_client::client;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_yuha_client_basic() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(30), async {
        // Start the Docker container with the SSH server
        let container = RemoteContainer::new().await?;
        let ssh_port = container.ssh_port().await?;

        // Connect to remote host
        println!("Connecting to 127.0.0.1:{} as root", ssh_port);
        let message_channel =
            client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

        println!("Connected to remote host successfully");

        // Create VscFreedevClient
        let client = yuha_client::YuhaClient::new(message_channel);
        println!("VscFreedevClient created successfully");

        // Test basic message exchange through the client
        let test_message = Bytes::from("Hello from VscFreedevClient");
        println!("Sending test message: {:?}", test_message);

        client.send_message(test_message.clone()).await?;
        println!("Message sent successfully");

        // Wait for response with timeout
        let response =
            tokio::time::timeout(Duration::from_secs(5), client.receive_message()).await??;

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
    })
    .await
    .map_err(|_| anyhow::anyhow!("Test timed out after 30 seconds"))?
}
