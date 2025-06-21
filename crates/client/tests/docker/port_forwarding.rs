#[cfg(feature = "docker-tests")]
use crate::shared;
#[cfg(feature = "docker-tests")]
use crate::shared::docker::RemoteContainer;
#[cfg(feature = "docker-tests")]
use crate::shared::test_utils::*;
#[cfg(feature = "docker-tests")]
use anyhow::Result;
#[cfg(feature = "docker-tests")]
use serial_test::serial;
#[cfg(feature = "docker-tests")]
use std::io::{Read, Write};
#[cfg(feature = "docker-tests")]
use std::net::TcpStream;
#[cfg(feature = "docker-tests")]
use std::time::Duration;
#[cfg(feature = "docker-tests")]
use yuha_client::simple_client;
#[cfg(feature = "docker-tests")]
use yuha_core::protocol::simple::ResponseItem;

#[cfg(feature = "docker-tests")]
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

#[cfg(feature = "docker-tests")]
#[tokio::test]
#[serial]
async fn test_port_forwarding_multiple_with_auto_upload() -> Result<()> {
    docker_test!();
    // Start the Docker container
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Connect to remote host with auto-upload
    let client = simple_client::connect_ssh_with_auto_upload(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
        true,
    )
    .await?;

    println!("Connected to remote host");

    // Setup multiple port forwards
    let port_pairs: Vec<(u16, u16)> = (0..3)
        .map(|_| (shared::get_random_port(), shared::get_random_port()))
        .collect();

    println!("Setting up {} port forwards", port_pairs.len());

    for (local_port, remote_port) in &port_pairs {
        println!(
            "Starting port forward: {} -> localhost:{}",
            local_port, remote_port
        );
        client
            .start_port_forward(*local_port, "localhost".to_string(), *remote_port)
            .await?;
        println!("Port forward {} started successfully", local_port);
    }

    println!("All port forwards started successfully");

    // Test that all port forwards are active by checking they exist
    // In a real test we'd verify actual connectivity, but for now we just
    // ensure the setup completed without errors

    println!("Multiple port forwarding test with auto-upload completed successfully");
    Ok(())
}

#[cfg(feature = "docker-tests")]
#[tokio::test]
#[serial]
async fn test_port_forwarding_stop_with_auto_upload() -> Result<()> {
    docker_test!();
    // Start the Docker container
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Connect to remote host with auto-upload
    let client = simple_client::connect_ssh_with_auto_upload(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
        true,
    )
    .await?;

    // Setup a port forward
    let local_port = shared::get_random_port();
    let remote_port = shared::get_random_port();

    println!(
        "Starting port forward: {} -> localhost:{}",
        local_port, remote_port
    );
    client
        .start_port_forward(local_port, "localhost".to_string(), remote_port)
        .await?;
    println!("Port forward started");

    // Now stop it
    println!("Stopping port forward on port {}", local_port);
    client.stop_port_forward(local_port).await?;
    println!("Port forward stopped");

    println!("Port forwarding stop test with auto-upload completed successfully");
    Ok(())
}

#[cfg(feature = "docker-tests")]
#[tokio::test]
#[serial]
async fn test_port_forwarding_to_echo_service() -> Result<()> {
    docker_test!();
    println!("Testing port forwarding to echo service");

    // Start the Docker container with the SSH server and echo service
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;
    let echo_port = container.echo_port().await?;

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

    // Setup port forward to the echo service
    let local_port = shared::get_random_port();
    println!(
        "Starting port forward: {} -> 127.0.0.1:{} (echo service)",
        local_port, echo_port
    );

    client
        .start_port_forward(local_port, "127.0.0.1".to_string(), echo_port)
        .await?;

    // Give the port forward time to establish
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Spawn a task to handle forwarding
    let client_clone = client.clone();
    let forward_task = tokio::spawn(async move {
        loop {
            match client_clone.poll_data().await {
                Ok(items) => {
                    for item in items {
                        if let ResponseItem::PortForwardData {
                            connection_id,
                            data,
                        } = item
                        {
                            println!(
                                "Forwarding {} bytes for connection {}",
                                data.len(),
                                connection_id
                            );
                            // Send the data back (it's an echo service)
                            if let Err(e) = client_clone
                                .send_port_forward_data(connection_id, data)
                                .await
                            {
                                println!("Failed to forward data: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    if !e.to_string().contains("timeout") {
                        println!("Poll error: {}", e);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    // Give forwarding time to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Test the echo service through the port forward
    println!("Connecting to localhost:{}", local_port);
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", local_port))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let test_message = "Hello, echo service!\n";
    println!("Sending test message: {}", test_message.trim());
    stream.write_all(test_message.as_bytes())?;
    stream.flush()?;

    // Read the echo response
    let mut buffer = vec![0u8; test_message.len()];
    println!("Waiting for echo response...");
    match stream.read_exact(&mut buffer) {
        Ok(_) => {
            let response = String::from_utf8_lossy(&buffer);
            println!("Received echo response: {}", response.trim());
            assert_eq!(response, test_message);
            println!("Echo test successful!");
        }
        Err(e) => {
            println!("Failed to read echo response: {}", e);
            return Err(e.into());
        }
    }

    // Clean up
    forward_task.abort();

    println!("Port forwarding to echo service test completed successfully");
    Ok(())
}
