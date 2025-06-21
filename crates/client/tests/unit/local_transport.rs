use anyhow::Result;
use serde_json;
use serial_test::serial;
use std::path::PathBuf;
use yuha_client::transport::{LocalTransport, LocalTransportConfig, TransportConfig};
use yuha_core::protocol::ProtocolRequest;

#[tokio::test]
#[serial]
async fn test_local_transport_creation() -> Result<()> {
    println!("Testing local transport creation");

    // Create transport with default binary path
    let config = LocalTransportConfig {
        binary_path: PathBuf::from("yuha-remote"),
        args: vec!["--stdio".to_string()],
    };
    let transport_config = TransportConfig::default();
    let _transport = LocalTransport::new(config, transport_config);
    println!("Created local transport with default binary path");

    println!("Local transport creation test passed!");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_local_transport_with_custom_binary() -> Result<()> {
    println!("Testing local transport with custom binary path");

    // Create transport with a non-existent binary path
    let config = LocalTransportConfig {
        binary_path: PathBuf::from("/nonexistent/binary"),
        args: vec!["--stdio".to_string()],
    };
    let transport_config = TransportConfig::default();
    let _transport = LocalTransport::new(config, transport_config);
    println!("Created transport with custom binary path");

    println!("Local transport custom binary test passed!");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_protocol_message_creation() -> Result<()> {
    println!("Testing protocol message creation");

    // Create various protocol messages
    let messages = vec![
        ProtocolRequest::PollData,
        ProtocolRequest::GetClipboard,
        ProtocolRequest::SetClipboard {
            content: "test".to_string(),
        },
        ProtocolRequest::StartPortForward {
            local_port: 8080,
            remote_host: "localhost".to_string(),
            remote_port: 80,
        },
        ProtocolRequest::StopPortForward { local_port: 8080 },
        ProtocolRequest::OpenBrowser {
            url: "https://example.com".to_string(),
        },
    ];

    for msg in messages {
        let json = serde_json::to_string(&msg)?;
        println!("Created message: {}", json);

        // Verify we can parse it back
        let _parsed: ProtocolRequest = serde_json::from_str(&json)?;
    }

    println!("Protocol message creation test passed!");
    Ok(())
}
