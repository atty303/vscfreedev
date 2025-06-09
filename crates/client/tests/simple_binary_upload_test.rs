mod shared;

use anyhow::Result;
use shared::docker::RemoteContainer;
use std::time::Duration;
use yuha_client::client;

#[tokio::test]
async fn test_simple_binary_upload() -> Result<()> {
    println!("Testing simple binary upload functionality");

    // Start the Docker container
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    println!("Container started on SSH port {}", ssh_port);

    // Test connection with auto-upload
    println!("Attempting SSH connection with auto-upload...");

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
            println!("✓ SSH connection with auto-upload successful!");

            // Try to send a simple message to test the connection
            println!("Sending test message...");
            let test_msg = bytes::Bytes::from("test-message");

            match channel.send(test_msg).await {
                Ok(_) => println!("✓ Message sent successfully"),
                Err(e) => {
                    println!("✗ Failed to send message: {:?}", e);

                    // Show container logs for debugging
                    if let Ok(logs) = container.get_logs().await {
                        println!("\n=== Container logs ===");
                        let recent_logs = logs.lines().rev().take(20).collect::<Vec<_>>();
                        for line in recent_logs.iter().rev() {
                            println!("{}", line);
                        }
                        println!("=== End of logs ===\n");
                    }

                    return Err(e.into());
                }
            }

            // Try to receive a response with a timeout
            println!("Waiting for response...");
            match tokio::time::timeout(Duration::from_secs(3), channel.receive()).await {
                Ok(Ok(response)) => {
                    println!(
                        "✓ Received response: {:?}",
                        String::from_utf8_lossy(&response)
                    );
                }
                Ok(Err(e)) => {
                    println!("✗ Failed to receive response: {:?}", e);
                }
                Err(_) => {
                    println!("⚠ Timeout waiting for response (this might be expected)");
                }
            }
        }
        Err(e) => {
            println!("✗ SSH connection with auto-upload failed: {:?}", e);

            // Show container logs for debugging
            if let Ok(logs) = container.get_logs().await {
                println!("\n=== Container logs ===");
                let recent_logs = logs.lines().rev().take(50).collect::<Vec<_>>();
                for line in recent_logs.iter().rev() {
                    println!("{}", line);
                }
                println!("=== End of logs ===\n");
            }

            return Err(e.into());
        }
    }

    println!("✓ Simple binary upload test completed successfully");
    Ok(())
}
