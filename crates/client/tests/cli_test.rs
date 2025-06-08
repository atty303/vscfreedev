mod shared;

use anyhow::Result;
use bytes::Bytes;
use std::time::Duration;
use tokio::time::sleep;
use shared::docker::RemoteContainer;
use vscfreedev_client::client;

#[tokio::test]
async fn test_ssh_connection() -> Result<()> {
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Wait a bit for the SSH server and remote executable to be fully ready
    sleep(Duration::from_secs(10)).await;

    // Connect directly using client::connect_ssh
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let mut message_channel = client::connect_ssh(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
    ).await?;

    println!("Connected to remote host");

    // Test message exchange
    let message = "Hello from E2E test!";
    println!("Sending message: {}", message);
    
    // Send message using MessageChannel
    if let Err(e) = message_channel.send(Bytes::from(message)).await {
        println!("MessageChannel send failed: {}", e);
        
        // Get and display container logs for debugging
        println!("---- Remote container logs ----");
        match container.get_logs().await {
            Ok(logs) => println!("{}", logs),
            Err(log_err) => println!("Error getting container logs: {}", log_err),
        }
        println!("---- End of remote container logs ----");
        
        return Err(anyhow::anyhow!("MessageChannel failed: {}", e));
    }

    // Add a small delay 
    sleep(Duration::from_secs(1)).await;

    let response = match tokio::time::timeout(Duration::from_secs(5), message_channel.receive()).await {
        Ok(result) => match result {
            Ok(msg) => msg,
            Err(e) => {
                println!("Error receiving response: {}", e);

                // Get and display container logs for debugging
                println!("---- Remote container logs ----");
                match container.get_logs().await {
                    Ok(logs) => println!("{}", logs),
                    Err(log_err) => println!("Error getting container logs: {}", log_err),
                }
                println!("---- End of remote container logs ----");

                return Err(anyhow::anyhow!("Failed to receive response: {}", e));
            }
        },
        Err(_) => {
            println!("Timeout waiting for response");

            // Get and display container logs for debugging
            println!("---- Remote container logs ----");
            match container.get_logs().await {
                Ok(logs) => println!("{}", logs),
                Err(log_err) => println!("Error getting container logs: {}", log_err),
            }
            println!("---- End of remote container logs ----");

            return Err(anyhow::anyhow!("Timeout waiting for response"));
        }
    };

    let response_str = String::from_utf8_lossy(&response);
    println!("Received response: {}", response_str);

    // Verify the response is an echo of our message
    assert_eq!(response_str, format!("Echo: {}", message), "Response should echo the sent message");

    println!("SSH connection test completed successfully");

    Ok(())
}
