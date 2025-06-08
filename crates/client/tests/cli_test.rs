mod shared;

use anyhow::Result;
use bytes::Bytes;
use std::time::Duration;
use tokio::time::sleep;
use shared::docker::RemoteContainer;
use vscfreedev_client::client;

#[tokio::test]
async fn test_ssh_connection() -> Result<()> {
    // Start the Docker container with the SSH server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Wait a bit for the SSH server and remote executable to be fully ready
    sleep(Duration::from_secs(10)).await;

    // Connect directly using client::connect_ssh
    println!("Connecting to 127.0.0.1:{} as root", ssh_port);
    let (mut channel, welcome_already_received) = client::connect_ssh(
        "127.0.0.1",
        ssh_port,
        "root",
        Some("password"),
        None,
    ).await?;

    println!("Connected to remote host");
    println!("Remote channel established");

    // If the welcome message hasn't already been received, try to receive it now
    if !welcome_already_received {
        println!("Welcome message not yet received, trying to receive it now");

        // Try to receive the welcome message with a timeout
        let welcome = match tokio::time::timeout(Duration::from_secs(20), channel.receive()).await {
            Ok(result) => match result {
                Ok(msg) => msg,
                Err(e) => {
                    println!("Error receiving welcome message: {}", e);

                    // Get and display container logs for debugging
                    println!("---- Remote container logs ----");
                    match container.get_logs().await {
                        Ok(logs) => println!("{}", logs),
                        Err(log_err) => println!("Error getting container logs: {}", log_err),
                    }
                    println!("---- End of remote container logs ----");

                    return Err(anyhow::anyhow!("Failed to receive welcome message: {}", e));
                }
            },
            Err(_) => {
                println!("Timeout waiting for welcome message");

                // Get and display container logs for debugging
                println!("---- Remote container logs ----");
                match container.get_logs().await {
                    Ok(logs) => println!("{}", logs),
                    Err(log_err) => println!("Error getting container logs: {}", log_err),
                }
                println!("---- End of remote container logs ----");

                return Err(anyhow::anyhow!("Timeout waiting for welcome message"));
            }
        };

        let welcome_str = String::from_utf8_lossy(&welcome);
        println!("Received welcome: {}", welcome_str);

        // Verify welcome message contains expected text
        assert!(welcome_str.contains("Welcome"), "Welcome message should contain 'Welcome'");
    } else {
        println!("Welcome message already received during connection");
    }

    // Send a message
    let message = "Hello from E2E test!";
    println!("Sending message: {}", message);
    if let Err(e) = channel.send(Bytes::from(message)).await {
        println!("Error sending message: {}", e);
        return Err(anyhow::anyhow!("Failed to send message: {}", e));
    }

    // Add a small delay to allow the message to be fully sent
    sleep(Duration::from_secs(1)).await;

    // Try to receive the response with a timeout
    let response = match tokio::time::timeout(Duration::from_secs(20), channel.receive()).await {
        Ok(result) => match result {
            Ok(msg) => msg,
            Err(e) => {
                println!("Error receiving response: {}", e);

                // Get and display container logs for debugging
                println!("---- Remote container logs ----");
                match container.get_logs().await {
                    Ok(logs) => println!("{}", logs),
                    Err(log_err) => println!("Error getting container logs: {}", log_err),
                }
                println!("---- End of remote container logs ----");

                return Err(anyhow::anyhow!("Failed to receive response: {}", e));
            }
        },
        Err(_) => {
            println!("Timeout waiting for response");

            // Get and display container logs for debugging
            println!("---- Remote container logs ----");
            match container.get_logs().await {
                Ok(logs) => println!("{}", logs),
                Err(log_err) => println!("Error getting container logs: {}", log_err),
            }
            println!("---- End of remote container logs ----");

            return Err(anyhow::anyhow!("Timeout waiting for response"));
        }
    };

    let response_str = String::from_utf8_lossy(&response);
    println!("Received response: {}", response_str);

    // Verify the response is an echo of our message
    assert_eq!(response_str, format!("Echo: {}", message), "Response should echo the sent message");

    println!("SSH connection test completed successfully");

    Ok(())
}
