//! Test for local transport implementation

use std::path::PathBuf;
use yuha_client::simple_client_transport::{SimpleYuhaClientTransport, connect_local};
use yuha_client::transport::{LocalTransport, LocalTransportConfig, TransportConfig};

#[tokio::test]
async fn test_local_transport_connection() {
    // Get the built remote binary path
    let remote_binary_path = PathBuf::from(yuha_client::client::get_remote_binary_path());

    // Create transport configuration
    let transport_config = TransportConfig::default();

    // Create local transport
    let local_config = LocalTransportConfig {
        binary_path: remote_binary_path.clone(),
        args: vec!["--stdio".to_string()],
    };

    let transport = LocalTransport::new(local_config, transport_config);
    let mut client = SimpleYuhaClientTransport::new(transport);

    // Test connection
    let result = client.connect().await;
    assert!(result.is_ok(), "Failed to connect: {:?}", result);

    // Test clipboard operation
    let test_content = "Hello from local transport test!";
    let result = client.set_clipboard(test_content.to_string()).await;
    assert!(result.is_ok(), "Failed to set clipboard: {:?}", result);

    let result = client.get_clipboard().await;
    assert!(result.is_ok(), "Failed to get clipboard: {:?}", result);
    assert_eq!(result.unwrap(), test_content);
}

#[tokio::test]
async fn test_local_transport_with_helper() {
    // Get the built remote binary path
    let remote_binary_path = PathBuf::from(yuha_client::client::get_remote_binary_path());

    // Test using the helper function
    let result = connect_local(remote_binary_path, TransportConfig::default()).await;
    assert!(result.is_ok(), "Failed to connect with helper");

    let client = result.unwrap();

    // Test browser operation
    let result = client.open_browser("https://example.com".to_string()).await;
    assert!(result.is_ok(), "Failed to open browser: {:?}", result);
}

#[tokio::test]
async fn test_local_transport_with_env_vars() {
    // Get the built remote binary path
    let remote_binary_path = PathBuf::from(yuha_client::client::get_remote_binary_path());

    // Create transport configuration with environment variables
    let mut transport_config = TransportConfig::default();
    transport_config
        .env_vars
        .push(("YUHA_TEST".to_string(), "local_transport".to_string()));

    // Create local transport
    let local_config = LocalTransportConfig {
        binary_path: remote_binary_path,
        args: vec!["--stdio".to_string()],
    };

    let transport = LocalTransport::new(local_config, transport_config);
    let mut client = SimpleYuhaClientTransport::new(transport);

    // Test connection
    let result = client.connect().await;
    assert!(result.is_ok(), "Failed to connect: {:?}", result);
}
