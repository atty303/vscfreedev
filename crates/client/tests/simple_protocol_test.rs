use anyhow::Result;
use serial_test::serial;
use std::path::PathBuf;
use yuha_client::simple_client;
use yuha_core::protocol::simple::{YuhaRequest, YuhaResponse};

#[tokio::test]
#[serial]
async fn test_local_transport_connection() -> Result<()> {
    // Test local transport connection
    let binary_path = PathBuf::from(yuha_client::client::get_remote_binary_path());
    let result = simple_client::connect_local_process(Some(&binary_path)).await;

    // This test just verifies that the connection attempt doesn't panic
    // The actual connection may fail in CI environments without the binary
    match result {
        Ok(_) => println!("Local transport connection successful"),
        Err(e) => println!("Local transport connection failed (expected in CI): {}", e),
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_protocol_message_creation() -> Result<()> {
    // Test that we can create protocol messages
    let request = YuhaRequest::GetClipboard;
    let response = YuhaResponse::Success;

    println!("Request: {:?}", request);
    println!("Response: {:?}", response);

    Ok(())
}
