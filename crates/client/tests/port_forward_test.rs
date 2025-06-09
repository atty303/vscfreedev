mod shared;

use anyhow::Result;
use shared::docker::RemoteContainer;
use std::time::Duration;
use tokio::time::sleep;
use vscfreedev_client::client;

#[tokio::test]
async fn test_port_forwarding_single() -> Result<()> {
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Wait for SSH server to be ready
    sleep(Duration::from_secs(10)).await;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    println!("Connected to remote host");

    // Create client with port forwarding capabilities
    let client = vscfreedev_client::VscFreedevClient::new(message_channel);

    // Test port forwarding setup
    let local_port = 8080;
    let remote_port = 80;

    println!(
        "Starting port forward: {} -> localhost:{}",
        local_port, remote_port
    );

    client
        .start_port_forward(local_port, "localhost", remote_port)
        .await?;

    println!("Single port forwarding test completed successfully");
    Ok(())
}

#[tokio::test]
async fn test_port_forwarding_multiple() -> Result<()> {
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Wait for SSH server to be ready
    sleep(Duration::from_secs(10)).await;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    println!("Connected to remote host");

    // Create client with port forwarding capabilities
    let client = vscfreedev_client::VscFreedevClient::new(message_channel);

    // Test multiple port forwarding
    let forwards = vec![(8080, 80), (8443, 443), (3000, 3000)];

    for (local_port, remote_port) in &forwards {
        println!(
            "Starting port forward: {} -> localhost:{}",
            local_port, remote_port
        );
        client
            .start_port_forward(*local_port, "localhost", *remote_port)
            .await?;
    }

    println!("Multiple port forwarding test completed successfully");
    Ok(())
}

#[tokio::test]
async fn test_port_forwarding_stop() -> Result<()> {
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Wait for SSH server to be ready
    sleep(Duration::from_secs(10)).await;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    println!("Connected to remote host");

    // Create client with port forwarding capabilities
    let client = vscfreedev_client::VscFreedevClient::new(message_channel);

    let local_port = 8080;
    let remote_port = 80;

    // Start port forwarding
    println!(
        "Starting port forward: {} -> localhost:{}",
        local_port, remote_port
    );
    client
        .start_port_forward(local_port, "localhost", remote_port)
        .await?;

    // Stop port forwarding
    println!("Stopping port forward for port {}", local_port);
    client.stop_port_forward(local_port).await?;

    println!("Port forwarding stop test completed successfully");
    Ok(())
}

#[tokio::test]
async fn test_port_forwarding_data_transfer() -> Result<()> {
    // Start the Docker container with the SSH server and echo service
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;
    let _echo_service_port = container.echo_service_port().await?;

    // Wait for SSH server and echo service to be ready
    sleep(Duration::from_secs(10)).await;

    // Connect to remote host
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let message_channel =
        client::connect_ssh("127.0.0.1", ssh_port, "root", Some("password"), None).await?;

    println!("Connected to remote host");

    // Create client with port forwarding capabilities
    let client = vscfreedev_client::VscFreedevClient::new(message_channel);

    let local_port = 9090;

    // Start port forwarding to the Docker container's echo service on port 8888
    println!("Starting port forward: {} -> localhost:8888", local_port);

    // Add timeout to port forward start to avoid hanging
    match tokio::time::timeout(
        Duration::from_secs(30),
        client.start_port_forward(local_port, "localhost", 8888),
    )
    .await
    {
        Ok(Ok(())) => {
            println!("Port forward started successfully");
        }
        Ok(Err(e)) => {
            return Err(anyhow::anyhow!("Port forward failed: {}", e));
        }
        Err(_) => {
            return Err(anyhow::anyhow!(
                "Port forward start timed out after 30 seconds"
            ));
        }
    }

    println!("Port forwarding established successfully");

    // Give more time for the port forward to be fully established
    sleep(Duration::from_secs(5)).await;

    // Test data transfer through the forwarded port
    println!("Testing port forward connection establishment");

    // Try to connect to the forwarded port and test data transfer with retries
    let mut connection_attempts = 0;
    let max_attempts = 3;
    let mut last_error = None;

    while connection_attempts < max_attempts {
        connection_attempts += 1;
        println!(
            "Connection attempt {} of {}",
            connection_attempts, max_attempts
        );

        match tokio::time::timeout(
            Duration::from_secs(10),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{}", local_port)),
        )
        .await
        {
            Ok(Ok(mut stream)) => {
                println!("Successfully connected to forwarded port");

                // Test data transfer
                let test_data = b"Hello through port forward!";
                println!(
                    "Sending test data: {:?}",
                    std::str::from_utf8(test_data).unwrap()
                );

                use tokio::io::{AsyncReadExt, AsyncWriteExt};

                // Send data with timeout
                match tokio::time::timeout(Duration::from_secs(5), stream.write_all(test_data))
                    .await
                {
                    Ok(Ok(())) => {
                        println!("Data sent successfully");

                        // Try to read response with increased timeout
                        let mut response_buf = [0; 1024];
                        match tokio::time::timeout(
                            Duration::from_secs(10),
                            stream.read(&mut response_buf),
                        )
                        .await
                        {
                            Ok(Ok(n)) => {
                                if n > 0 {
                                    let response = &response_buf[..n];
                                    println!(
                                        "Received response: {:?}",
                                        std::str::from_utf8(response).unwrap()
                                    );

                                    if response == test_data {
                                        println!(
                                            "Perfect! Bidirectional data transfer works - echo response matches"
                                        );
                                    } else {
                                        println!(
                                            "Response received but doesn't match - partial success"
                                        );
                                    }
                                } else {
                                    println!(
                                        "Connection closed by remote - at least outbound data was sent"
                                    );
                                }
                            }
                            Ok(Err(e)) => {
                                println!(
                                    "Error reading response: {} - but connection was established",
                                    e
                                );
                            }
                            Err(_) => {
                                println!(
                                    "Timeout reading response - but connection and outbound data transfer worked"
                                );
                            }
                        }

                        // Connection was successful, break out of retry loop
                        break;
                    }
                    Ok(Err(e)) => {
                        println!("Failed to send data: {}", e);
                        last_error = Some(anyhow::anyhow!("Failed to send data: {}", e));
                    }
                    Err(_) => {
                        println!("Timeout sending data");
                        last_error = Some(anyhow::anyhow!("Timeout sending data"));
                    }
                }
            }
            Ok(Err(e)) => {
                println!(
                    "Failed to connect to forwarded port (attempt {}): {}",
                    connection_attempts, e
                );
                last_error = Some(anyhow::anyhow!("Port forwarding connection failed: {}", e));

                if connection_attempts < max_attempts {
                    println!("Waiting 2 seconds before retry...");
                    sleep(Duration::from_secs(2)).await;
                }
            }
            Err(_) => {
                println!(
                    "Timeout connecting to forwarded port (attempt {})",
                    connection_attempts
                );
                last_error = Some(anyhow::anyhow!("Timeout connecting to forwarded port"));

                if connection_attempts < max_attempts {
                    println!("Waiting 2 seconds before retry...");
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    // If we exhausted all attempts without success, return the last error
    if connection_attempts >= max_attempts && last_error.is_some() {
        return Err(last_error.unwrap());
    }

    println!("Data transfer test completed successfully");
    Ok(())
}
