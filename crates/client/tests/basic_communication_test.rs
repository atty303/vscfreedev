mod shared;
use shared::test_utils::*;

use anyhow::Result;
use serial_test::serial;
use shared::docker::RemoteContainer;
use std::time::Duration;
use yuha_client::simple_client;
use yuha_core::protocol::simple::ResponseItem;

#[tokio::test]
#[serial]
async fn test_basic_local_communication() -> Result<()> {
    let test_start = std::time::Instant::now();
    println!(
        "Basic local communication test started at: {:?}",
        test_start
    );

    // Connect to local process
    println!("Connecting to local yuha-remote process");
    let connect_start = std::time::Instant::now();

    let client = match simple_client::connect_local_process(None).await {
        Ok(client) => {
            println!("Local connection successful!");
            client
        }
        Err(e) => {
            println!("Local connection failed: {:?}", e);
            return Err(e.into());
        }
    };

    let connected = std::time::Instant::now();
    println!(
        "Local connection completed in: {:?}",
        connected.duration_since(connect_start)
    );

    // Test clipboard functionality
    let test_content = "Hello from local process!";
    println!("Setting clipboard content: {}", test_content);

    match client.set_clipboard(test_content.to_string()).await {
        Ok(_) => println!("Clipboard set successfully"),
        Err(e) => {
            println!("Failed to set clipboard: {:?}", e);
            return Err(e.into());
        }
    }

    // Small delay to ensure clipboard is set
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Get clipboard content
    match client.get_clipboard().await {
        Ok(content) => {
            println!("Retrieved clipboard content: {}", content);
            assert_eq!(content, test_content);
        }
        Err(e) => {
            println!("Failed to get clipboard: {:?}", e);
            return Err(e.into());
        }
    }

    // Test browser open functionality
    let test_url = "https://example.com";
    println!("Opening browser with URL: {}", test_url);
    client.open_browser(test_url.to_string()).await?;

    println!("Basic local communication test completed successfully");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_basic_ssh_communication_with_auto_upload() -> Result<()> {
    docker_test!();
    let test_start = std::time::Instant::now();
    println!("Basic test with auto-upload started at: {:?}", test_start);

    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let container_ready = std::time::Instant::now();
    println!(
        "Basic test - Docker container ready in: {:?}",
        container_ready.duration_since(test_start)
    );

    let ssh_port = container.ssh_port().await?;

    // Connect to remote host with auto-upload enabled
    println!(
        "Connecting to 127.0.0.1:{} as root with auto-upload",
        ssh_port
    );
    let ssh_connect_start = std::time::Instant::now();

    let client = match simple_client::connect_ssh_with_auto_upload(
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
            println!("SSH connection with auto-upload successful!");
            client
        }
        Err(e) => {
            println!("SSH connection with auto-upload failed: {:?}", e);
            // Get container logs for debugging
            if let Ok(logs) = container.get_logs().await {
                println!("=== Container logs (connection failed) ===");
                println!("{}", logs);
                println!("=== End of logs ===");
            }
            return Err(e.into());
        }
    };

    let ssh_connected = std::time::Instant::now();
    println!(
        "Basic test - SSH connection completed in: {:?}",
        ssh_connected.duration_since(ssh_connect_start)
    );
    println!(
        "Basic test - Total time to SSH ready: {:?}",
        ssh_connected.duration_since(test_start)
    );

    // Test clipboard functionality
    let test_content = "Hello from auto-uploaded binary!";
    println!("Setting clipboard content: {}", test_content);

    match client.set_clipboard(test_content.to_string()).await {
        Ok(_) => println!("Clipboard set successfully"),
        Err(e) => {
            println!("Failed to set clipboard: {:?}", e);
            // Get container logs for debugging
            if let Ok(logs) = container.get_logs().await {
                println!("=== Container logs ===");
                println!("{}", logs);
                println!("=== End of logs ===");
            }
            return Err(e.into());
        }
    }

    // Small delay to ensure clipboard is set
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Get clipboard content
    match client.get_clipboard().await {
        Ok(content) => {
            println!("Retrieved clipboard content: {}", content);
            assert_eq!(content, test_content);
        }
        Err(e) => {
            println!("Failed to get clipboard: {:?}", e);
            // Get container logs for debugging
            if let Ok(logs) = container.get_logs().await {
                println!("=== Container logs ===");
                println!("{}", logs);
                println!("=== End of logs ===");
            }
            return Err(e.into());
        }
    }

    // Test browser open functionality
    let test_url = "https://example.com";
    println!("Opening browser with URL: {}", test_url);
    client.open_browser(test_url.to_string()).await?;

    println!("Basic communication test with auto-upload completed successfully");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_polling_mechanism() -> Result<()> {
    println!("Testing polling mechanism with local process");

    // Connect to local process first (faster than SSH)
    let client = simple_client::connect_local_process(None).await?;

    // Test polling for data
    let mut received_count = 0;
    let start_time = std::time::Instant::now();

    while start_time.elapsed() < Duration::from_secs(2) {
        match client.poll_data().await {
            Ok(items) => {
                for item in items {
                    match item {
                        ResponseItem::ClipboardContent { .. } => {
                            println!("Received clipboard content via polling");
                            received_count += 1;
                        }
                        ResponseItem::PortForwardData { .. } => {
                            println!("Received port forward data via polling");
                            received_count += 1;
                        }
                        ResponseItem::NewConnection {
                            connection_id,
                            local_port,
                        } => {
                            println!(
                                "Received new connection: id={}, port={}",
                                connection_id, local_port
                            );
                            received_count += 1;
                        }
                        ResponseItem::CloseConnection { connection_id } => {
                            println!("Received close connection: id={}", connection_id);
                            received_count += 1;
                        }
                    }
                }
            }
            Err(e) => {
                println!("Polling error (expected during quiet periods): {}", e);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    println!("Polling test completed, received {} items", received_count);
    Ok(())
}
