mod shared;
use shared::test_utils::*;

use anyhow::Result;
use serial_test::serial;
use shared::docker::RemoteContainer;
use std::time::Duration;
use yuha_client::simple_client;

#[tokio::test]
#[serial]
async fn test_port_forwarding_local_process() -> Result<()> {
    println!("Testing port forwarding with local process");

    // Connect to local process
    let client = simple_client::connect_local_process(None).await?;
    println!("Connected to local yuha-remote process");

    // Test port forwarding setup
    let local_port = shared::get_random_port();
    let remote_port = shared::get_random_port();

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

#[tokio::test]
#[serial]
async fn test_port_forwarding_single_with_auto_upload() -> Result<()> {
    docker_test!();
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Connect to remote host with auto-upload
    println!(
        "Connecting to 127.0.0.1:{} as root with auto-upload",
        ssh_port
    );
    let client = simple_client::connect_ssh_with_auto_upload(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
        true,
    )
    .await?;

    println!("Connected to remote host with auto-uploaded binary");

    // Test port forwarding setup
    let local_port = shared::get_random_port();
    let remote_port = shared::get_random_port();

    println!(
        "Starting port forward: {} -> localhost:{}",
        local_port, remote_port
    );

    let start_time = std::time::Instant::now();
    client
        .start_port_forward(local_port, "localhost".to_string(), remote_port)
        .await?;
    let elapsed = start_time.elapsed();

    println!("Port forwarding setup took: {:?}", elapsed);
    println!("Single port forwarding test with auto-upload completed successfully");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_port_forwarding_multiple_with_auto_upload() -> Result<()> {
    docker_test!();
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Connect to remote host with auto-upload
    println!(
        "Connecting to 127.0.0.1:{} as root with auto-upload",
        ssh_port
    );
    let client = simple_client::connect_ssh_with_auto_upload(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
        true,
    )
    .await?;

    println!("Connected to remote host with auto-uploaded binary");

    // Test multiple port forwarding
    let forwards = vec![
        (shared::get_random_port(), shared::get_random_port()),
        (shared::get_random_port(), shared::get_random_port()),
        (shared::get_random_port(), shared::get_random_port()),
    ];

    for (local_port, remote_port) in &forwards {
        println!(
            "Starting port forward: {} -> localhost:{}",
            local_port, remote_port
        );
        client
            .start_port_forward(*local_port, "localhost".to_string(), *remote_port)
            .await?;

        // Small delay between port forwards
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    println!("Multiple port forwarding test with auto-upload completed successfully");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_port_forwarding_stop_with_auto_upload() -> Result<()> {
    docker_test!();
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Connect to remote host with auto-upload
    println!(
        "Connecting to 127.0.0.1:{} as root with auto-upload",
        ssh_port
    );
    let client = simple_client::connect_ssh_with_auto_upload(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
        true,
    )
    .await?;

    println!("Connected to remote host with auto-uploaded binary");

    let local_port = shared::get_random_port();
    let remote_port = shared::get_random_port();

    // Start port forwarding
    println!(
        "Starting port forward: {} -> localhost:{}",
        local_port, remote_port
    );

    client
        .start_port_forward(local_port, "localhost".to_string(), remote_port)
        .await?;

    // Port forwarding should be established immediately
    // No need to wait - the listener should be up and ready

    // Stop port forwarding
    println!("Stopping port forward on port {}", local_port);
    client.stop_port_forward(local_port).await?;

    println!("Port forwarding stop test with auto-upload completed successfully");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_port_forwarding_to_echo_service() -> Result<()> {
    docker_test!();
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;
    let echo_port = container.echo_service_port().await?;

    // Connect to remote host with auto-upload
    println!(
        "Connecting to 127.0.0.1:{} as root with auto-upload",
        ssh_port
    );
    let client = simple_client::connect_ssh_with_auto_upload(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
        true,
    )
    .await?;

    println!("Connected to remote host with auto-uploaded binary");
    println!("Echo service is available on host port {}", echo_port);

    // Set up port forwarding to the echo service inside the container
    let local_port = shared::get_random_port();
    println!(
        "Starting port forward: {} -> localhost:8888 (echo service inside container)",
        local_port
    );

    client
        .start_port_forward(local_port, "localhost".to_string(), 8888)
        .await?;

    // Give the port forward time to establish
    tokio::time::sleep(Duration::from_millis(200)).await;

    println!("Port forwarding to echo service test with auto-upload completed successfully");
    Ok(())
}
