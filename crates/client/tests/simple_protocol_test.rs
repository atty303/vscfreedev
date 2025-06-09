use anyhow::Result;
use tokio::io::duplex;
use yuha_client::simple_client::SimpleYuhaClient;
use yuha_core::message_channel::MessageChannel;
use yuha_core::protocol::{YuhaRequest, YuhaResponse};

#[tokio::test]
async fn test_simple_protocol_compilation() -> Result<()> {
    // Simple test to verify compilation and basic functionality
    let (client_stream, _server_stream) = duplex(8192);
    let client_channel = MessageChannel::new_with_stream(client_stream);
    let _client = SimpleYuhaClient::new(client_channel);

    println!("Simple protocol implementation compiled successfully");
    Ok(())
}

#[tokio::test]
async fn test_protocol_message_creation() -> Result<()> {
    // Test that we can create protocol messages
    let request = YuhaRequest::GetClipboard;
    let response = YuhaResponse::Success;

    println!("Request: {:?}", request);
    println!("Response: {:?}", response);

    Ok(())
}
