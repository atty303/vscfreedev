use anyhow::Result;
use yuha_core::protocol::YuhaRequest;

#[tokio::test]
async fn test_protocol_message_serialization() -> Result<()> {
    // Test basic message serialization
    let start_request = YuhaRequest::StartPortForward {
        local_port: 8080,
        remote_host: "localhost".to_string(),
        remote_port: 80,
    };

    let json = serde_json::to_string(&start_request)?;
    println!("StartPortForward JSON: {}", json);

    let parsed: YuhaRequest = serde_json::from_str(&json)?;
    match parsed {
        YuhaRequest::StartPortForward {
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
