mod shared;

use anyhow::Result;
use shared::docker::RemoteContainer;
use std::time::Duration;
use yuha_client::client;

#[tokio::test]
async fn test_binary_transfer_and_execution() -> Result<()> {
    println!("Testing binary transfer and execution");

    // Start the Docker container
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    println!("Container started on port {}", ssh_port);

    // First, test without auto-upload to see container status
    println!("\n=== Testing connection without auto-upload ===");
    match client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await {
        Ok(_) => {
            println!("ERROR: Connection succeeded without binary - this should fail!");
            // Get container logs
            if let Ok(logs) = container.get_logs().await {
                println!("=== Container logs (no binary) ===");
                println!("{}", logs);
                println!("=== End of logs ===");
            }
        }
        Err(e) => {
            println!("Expected error without binary: {:?}", e);
        }
    }

    // Now test with auto-upload
    println!("\n=== Testing connection with auto-upload ===");
    match client::connect_ssh_with_options(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
        true, // Enable auto-upload
    )
    .await
    {
        Ok(mut channel) => {
            println!("Connection succeeded with auto-upload!");

            // Try a simple message exchange
            let test_msg = bytes::Bytes::from("ping");
            println!("Sending test message: {:?}", test_msg);

            match channel.send(test_msg).await {
                Ok(_) => println!("Message sent successfully"),
                Err(e) => println!("Failed to send message: {:?}", e),
            }

            // Try to receive response with timeout
            match tokio::time::timeout(Duration::from_secs(2), channel.receive()).await {
                Ok(Ok(response)) => {
                    println!(
                        "Received response: {:?}",
                        String::from_utf8_lossy(&response)
                    );
                }
                Ok(Err(e)) => {
                    println!("Failed to receive response: {:?}", e);
                }
                Err(_) => {
                    println!("Timeout waiting for response");
                }
            }

            // Get container logs
            if let Ok(logs) = container.get_logs().await {
                println!("\n=== Container logs (with binary) ===");
                println!("{}", logs);
                println!("=== End of logs ===");
            }
        }
        Err(e) => {
            println!("Connection failed with auto-upload: {:?}", e);

            // Get container logs for debugging
            if let Ok(logs) = container.get_logs().await {
                println!("\n=== Container logs (failed) ===");
                println!("{}", logs);
                println!("=== End of logs ===");
            }
            return Err(e.into());
        }
    }

    println!("\nBinary transfer test completed");
    Ok(())
}
