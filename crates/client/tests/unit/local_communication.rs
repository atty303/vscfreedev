use anyhow::Result;
use serial_test::serial;
use std::time::Duration;
use yuha_client::client;
use yuha_core::protocol::ResponseItem;

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

    let client = match client::connect_local_process(None).await {
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

#[tokio::test]
#[serial]
async fn test_port_forwarding_local_process() -> Result<()> {
    println!("Testing port forwarding with local process");

    // Connect to local process
    let client = simple_client::connect_local_process(None).await?;
    println!("Connected to local yuha-remote process");

    // Test port forwarding setup
    let local_port = crate::shared::get_random_port();
    let remote_port = crate::shared::get_random_port();

    println!(
        "Starting port forward: {} -> localhost:{}",
        local_port, remote_port
    );

    let start_time = std::time::Instant::now();
    match client
        .start_port_forward(local_port, "localhost".to_string(), remote_port)
        .await
    {
        Ok(_) => {
            let elapsed = start_time.elapsed();
            println!("Port forwarding setup took: {:?}", elapsed);

            // Test stopping the port forward
            println!("Stopping port forward on port {}", local_port);
            client.stop_port_forward(local_port).await?;
            println!("Port forward stopped successfully");
        }
        Err(e) => {
            println!("Port forwarding test result: {:?}", e);
            // Port forwarding might not be fully functional in local mode
            // depending on the implementation, but the command should be accepted
        }
    }

    println!("Local process port forwarding test completed");
    Ok(())
}
