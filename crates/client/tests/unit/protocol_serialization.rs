use anyhow::Result;
use serde_json;
use serial_test::serial;
use yuha_core::protocol::simple::{ResponseItem, SimpleRequest, SimpleResponse};

#[tokio::test]
#[serial]
async fn test_protocol_message_serialization() -> Result<()> {
    // Test basic message serialization
    let start_request = SimpleRequest::StartPortForward {
        local_port: 8080,
        remote_host: "localhost".to_string(),
        remote_port: 80,
    };

    let json = serde_json::to_string(&start_request)?;
    println!("StartPortForward JSON: {}", json);

    let parsed: SimpleRequest = serde_json::from_str(&json)?;
    match parsed {
        SimpleRequest::StartPortForward {
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

#[tokio::test]
#[serial]
async fn test_all_request_types_serialization() -> Result<()> {
    // Test all request types
    let requests = vec![
        SimpleRequest::PollData,
        SimpleRequest::StartPortForward {
            local_port: 8080,
            remote_host: "example.com".to_string(),
            remote_port: 443,
        },
        SimpleRequest::StopPortForward { local_port: 8080 },
        SimpleRequest::PortForwardData {
            connection_id: 123,
            data: bytes::Bytes::from(vec![1, 2, 3, 4, 5]),
        },
        SimpleRequest::GetClipboard,
        SimpleRequest::SetClipboard {
            content: "test clipboard".to_string(),
        },
        SimpleRequest::OpenBrowser {
            url: "https://example.com".to_string(),
        },
    ];

    for request in requests {
        let json = serde_json::to_string(&request)?;
        let parsed: SimpleRequest = serde_json::from_str(&json)?;

        // Verify round-trip serialization
        let json2 = serde_json::to_string(&parsed)?;
        assert_eq!(json, json2, "Request serialization mismatch");
    }

    println!("All request types serialization test passed!");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_response_types_serialization() -> Result<()> {
    // Test all response types
    let responses = vec![
        SimpleResponse::Success,
        SimpleResponse::Error {
            message: "Test error".to_string(),
        },
        SimpleResponse::Data {
            items: vec![
                ResponseItem::ClipboardContent {
                    content: "clipboard data".to_string(),
                },
                ResponseItem::PortForwardData {
                    connection_id: 456,
                    data: bytes::Bytes::from(vec![10, 20, 30]),
                },
                ResponseItem::NewConnection {
                    connection_id: 789,
                    local_port: 8080,
                },
                ResponseItem::CloseConnection { connection_id: 789 },
            ],
        },
    ];

    for response in responses {
        let json = serde_json::to_string(&response)?;
        let parsed: SimpleResponse = serde_json::from_str(&json)?;

        // Verify round-trip serialization
        let json2 = serde_json::to_string(&parsed)?;
        assert_eq!(json, json2, "Response serialization mismatch");
    }

    println!("All response types serialization test passed!");
    Ok(())
}
