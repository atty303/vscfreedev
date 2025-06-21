#[cfg(feature = "docker-tests")]
use crate::shared::docker::RemoteContainer;
#[cfg(feature = "docker-tests")]
use crate::shared::test_utils::*;
#[cfg(feature = "docker-tests")]
use anyhow::Result;
#[cfg(feature = "docker-tests")]
use serial_test::serial;
#[cfg(feature = "docker-tests")]
use yuha_client::client;

#[cfg(feature = "docker-tests")]
#[tokio::test]
#[serial]
async fn test_binary_transfer_and_execution() -> Result<()> {
    docker_test!();
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // First, try to connect without auto-upload (should fail)
    println!("Attempting connection without auto-upload (expecting failure)");
    let result = client::connect_ssh_with_auto_upload(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
        false, // No auto-upload
    )
    .await;

    match result {
        Err(e) => {
            println!("Connection failed as expected: {}", e);
            assert!(
                e.to_string().contains("yuha-remote")
                    || e.to_string().contains("not found")
                    || e.to_string().contains("Remote command failed"),
                "Expected error about missing yuha-remote, got: {}",
                e
            );
        }
        Ok(_) => {
            panic!("Connection should have failed without yuha-remote binary!");
        }
    }

    // Now connect with auto-upload (should succeed)
    println!("Attempting connection with auto-upload");
    let client = client::connect_ssh_with_auto_upload(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
        true, // Enable auto-upload
    )
    .await?;

    println!("Successfully connected with auto-uploaded binary");

    // Verify the binary is working by performing a simple operation
    let test_content = "Binary upload successful!";
    client.set_clipboard(test_content.to_string()).await?;
    let retrieved = client.get_clipboard().await?;
    assert_eq!(retrieved, test_content);

    println!("Binary transfer and execution test completed successfully");
    Ok(())
}

#[cfg(feature = "docker-tests")]
#[tokio::test]
#[serial]
async fn test_simple_binary_upload() -> Result<()> {
    docker_test!();
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    println!(
        "Testing simple binary upload to 127.0.0.1:{} as root",
        ssh_port
    );

    let client = client::connect_ssh_with_auto_upload(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
        true,
    )
    .await?;

    println!("Binary upload test successful");

    // Test basic functionality to ensure the binary is working
    client.get_clipboard().await?;

    Ok(())
}
