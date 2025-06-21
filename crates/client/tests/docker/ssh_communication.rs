#[cfg(feature = "docker-tests")]
use crate::shared::docker::RemoteContainer;
#[cfg(feature = "docker-tests")]
use crate::shared::test_utils::*;
#[cfg(feature = "docker-tests")]
use anyhow::Result;
#[cfg(feature = "docker-tests")]
use serial_test::serial;
#[cfg(feature = "docker-tests")]
use std::time::Duration;
#[cfg(feature = "docker-tests")]
use yuha_client::simple_client;

/// Tests basic SSH communication with automatic binary upload.
///
/// This integration test verifies that the SSH transport can:
/// - Establish a connection to a remote SSH server
/// - Automatically detect that the yuha-remote binary is missing
/// - Upload the binary to the remote server
/// - Execute the binary and establish protocol communication
/// - Perform basic request-response operations
///
/// **Expected Duration**: < 15 seconds
/// **Dependencies**: Docker daemon must be running
/// **Test Type**: Integration test with real SSH server
///
/// # Test Flow
///
/// 1. Start Docker container with SSH server
/// 2. Connect via SSH transport with auto-upload enabled
/// 3. Verify binary is uploaded and executed
/// 4. Test basic protocol operations (clipboard, browser)
/// 5. Verify connection cleanup
#[cfg(feature = "docker-tests")]
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
