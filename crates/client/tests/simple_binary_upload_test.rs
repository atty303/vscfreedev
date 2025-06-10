mod shared;
use shared::test_utils::*;

use anyhow::Result;
use shared::docker::RemoteContainer;
use std::time::Duration;
use yuha_client::simple_client;

#[tokio::test]
async fn test_local_process_startup() -> Result<()> {
    println!("Testing local process startup (no upload needed)");

    // Test local process connection
    println!("Attempting local process connection...");

    match simple_client::connect_local_process(None).await {
        Ok(client) => {
            println!("✓ Local process connection successful!");

            // Test basic functionality
            println!("Testing basic clipboard functionality...");
            let test_content = "Local process test";

            match client.set_clipboard(test_content.to_string()).await {
                Ok(_) => {
                    println!("✓ Clipboard set successfully");

                    // Small delay
                    tokio::time::sleep(Duration::from_millis(100)).await;

                    match client.get_clipboard().await {
                        Ok(content) => {
                            println!("✓ Retrieved clipboard: {}", content);
                            assert_eq!(content, test_content);
                        }
                        Err(e) => {
                            println!("✗ Failed to get clipboard: {:?}", e);
                            return Err(e.into());
                        }
                    }
                }
                Err(e) => {
                    println!("✗ Failed to set clipboard: {:?}", e);
                    return Err(e.into());
                }
            }

            // Test browser functionality
            println!("Testing browser functionality...");
            match client.open_browser("https://example.com".to_string()).await {
                Ok(_) => println!("✓ Browser opened successfully"),
                Err(e) => {
                    println!("✗ Failed to open browser: {:?}", e);
                    return Err(e.into());
                }
            }
        }
        Err(e) => {
            println!("✗ Local process connection failed: {:?}", e);
            return Err(e.into());
        }
    }

    println!("✓ Local process startup test completed successfully");
    Ok(())
}

#[tokio::test]
async fn test_simple_binary_upload() -> Result<()> {
    docker_test!();
    println!("Testing simple binary upload functionality");

    // Start the Docker container
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    println!("Container started on SSH port {}", ssh_port);

    // Test connection with auto-upload
    println!("Attempting SSH connection with auto-upload...");

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
            println!("✓ SSH connection with auto-upload successful!");

            // Test basic functionality
            println!("Testing clipboard functionality...");
            let test_content = "SSH upload test";

            match client.set_clipboard(test_content.to_string()).await {
                Ok(_) => {
                    println!("✓ Clipboard set successfully");

                    // Small delay to ensure operation completes
                    tokio::time::sleep(Duration::from_millis(200)).await;

                    match client.get_clipboard().await {
                        Ok(content) => {
                            println!("✓ Retrieved clipboard: {}", content);
                            assert_eq!(content, test_content);
                        }
                        Err(e) => {
                            println!("✗ Failed to get clipboard: {:?}", e);

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
                }
                Err(e) => {
                    println!("✗ Failed to set clipboard: {:?}", e);

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

            // Test browser functionality
            println!("Testing browser functionality...");
            match client.open_browser("https://example.com".to_string()).await {
                Ok(_) => println!("✓ Browser opened successfully"),
                Err(e) => {
                    println!("✗ Failed to open browser: {:?}", e);
                    return Err(e.into());
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
