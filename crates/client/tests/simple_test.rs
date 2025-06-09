use anyhow::Result;
use yuha_core::port_forward::PortForwardMessage;

#[tokio::test]
async fn test_port_forward_message_serialization() -> Result<()> {
    // Test basic message serialization
    let start_request = PortForwardMessage::StartRequest {
        local_port: 8080,
        remote_host: "localhost".to_string(),
        remote_port: 80,
    };

    let json = serde_json::to_string(&start_request)?;
    println!("StartRequest JSON: {}", json);

    let parsed: PortForwardMessage = serde_json::from_str(&json)?;
    match parsed {
        PortForwardMessage::StartRequest {
            local_port,
            remote_host,
            remote_port,
        } => {
            assert_eq!(local_port, 8080);
            assert_eq!(remote_host, "localhost");
            assert_eq!(remote_port, 80);
        }
        _ => panic!("Wrong message type parsed"),
    }

    println!("Message serialization test passed!");
    Ok(())
}
