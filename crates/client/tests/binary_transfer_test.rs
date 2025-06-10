mod shared;
use shared::test_utils::*;

use anyhow::Result;
use shared::docker::RemoteContainer;
use std::time::Duration;
use yuha_client::simple_client;

#[tokio::test]
async fn test_local_process_execution() -> Result<()> {
    println!("Testing local process execution");

    // Test local process connection (no binary transfer needed)
    println!("\n=== Testing local process connection ===");
    match simple_client::connect_local_process(None).await {
        Ok(client) => {
            println!("Local connection succeeded!");

            // Test clipboard functionality
            let test_content = "Binary execution test - local";
            println!("Testing clipboard with: {}", test_content);

            client.set_clipboard(test_content.to_string()).await?;
            let retrieved = client.get_clipboard().await?;

            assert_eq!(retrieved, test_content);
            println!("Clipboard test successful!");

            // Test browser functionality
            println!("Testing browser functionality");
            client
                .open_browser("https://example.com".to_string())
                .await?;
            println!("Browser test successful!");
        }
        Err(e) => {
            println!("Local connection failed: {:?}", e);
            return Err(e.into());
        }
    }

    println!("\nLocal process execution test completed");
    Ok(())
}

#[tokio::test]
async fn test_binary_transfer_and_execution() -> Result<()> {
    docker_test!();
    println!("Testing binary transfer and execution");

    // Start the Docker container
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    println!("Container started on port {}", ssh_port);

    // First, test without auto-upload to see container status
    println!("\n=== Testing connection without auto-upload ===");
    match simple_client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await {
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
    match simple_client::connect_ssh_with_auto_upload(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
        true, // Enable auto-upload
    )
    .await
    {
        Ok(client) => {
            println!("Connection succeeded with auto-upload!");

            // Test clipboard functionality
            let test_content = "Binary transfer test - SSH";
            println!("Testing clipboard with: {}", test_content);

            client.set_clipboard(test_content.to_string()).await?;

            // Small delay to ensure the operation completes
            tokio::time::sleep(Duration::from_millis(100)).await;

            let retrieved = client.get_clipboard().await?;
            assert_eq!(retrieved, test_content);
            println!("Clipboard test successful!");

            // Test browser functionality
            println!("Testing browser functionality");
            client
                .open_browser("https://example.com".to_string())
                .await?;
            println!("Browser test successful!");

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
