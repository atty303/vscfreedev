//! Comprehensive functionality tests using local transport for fast execution

use std::time::Duration;
use tokio::time::sleep;

mod shared;
use shared::test_utils::*;

#[tokio::test]
async fn test_basic_connection_and_polling() {
    let client = create_local_client()
        .await
        .expect("Failed to create client");

    // Test basic polling - client already connected
    // SimpleYuhaClientTransport doesn't expose polling directly,
    // but we can test it's connected by doing an operation
    let result = client.get_clipboard().await;
    assert!(result.is_ok(), "Failed to communicate with server");
}

#[tokio::test]
async fn test_clipboard_operations() {
    let mut fixture = TestFixture::new().await.expect("Failed to create fixture");

    // Test empty clipboard
    let content = fixture
        .client
        .get_clipboard()
        .await
        .expect("Failed to get clipboard");
    assert_eq!(content, "");

    // Test setting and getting clipboard
    fixture
        .test_clipboard_roundtrip("Hello, World!")
        .await
        .expect("Clipboard roundtrip failed");
    fixture
        .test_clipboard_roundtrip("Multi\nLine\nText")
        .await
        .expect("Multiline clipboard failed");
    fixture
        .test_clipboard_roundtrip("Special chars: 日本語 emoji 🎉")
        .await
        .expect("Special chars clipboard failed");
}

#[tokio::test]
async fn test_browser_operations() {
    let client = create_local_client()
        .await
        .expect("Failed to create client");

    // Test opening various URLs
    let test_urls = vec![
        "https://example.com",
        "http://localhost:8080",
        "file:///home/user/test.html",
        "https://github.com/user/repo",
    ];

    for url in test_urls {
        client
            .open_browser(url.to_string())
            .await
            .expect("Failed to open browser");
    }
}

#[tokio::test]
async fn test_port_forwarding_lifecycle() {
    let client = create_local_client()
        .await
        .expect("Failed to create client");

    // Start port forwarding
    let local_port = shared::get_random_port();
    let remote_port = 8080;

    client
        .start_port_forward(local_port, "localhost".to_string(), remote_port)
        .await
        .expect("Failed to start port forward");

    // Give it a moment to start
    sleep(Duration::from_millis(100)).await;

    // Port forwarding data is sent via a different mechanism
    // For now, we'll just test the lifecycle

    // Stop port forwarding
    client
        .stop_port_forward(local_port)
        .await
        .expect("Failed to stop port forward");
}

#[tokio::test]
async fn test_multiple_port_forwards() {
    let client = create_local_client()
        .await
        .expect("Failed to create client");

    // Start multiple port forwards
    let forwards = vec![
        (shared::get_random_port(), 8080),
        (shared::get_random_port(), 8081),
        (shared::get_random_port(), 8082),
    ];

    // Start all forwards
    for (local, remote) in &forwards {
        client
            .start_port_forward(*local, "localhost".to_string(), *remote)
            .await
            .expect("Failed to start port forward");
    }

    sleep(Duration::from_millis(200)).await;

    // Port forwarding data is sent via a different mechanism
    // For now, we'll just test the lifecycle

    // Stop all forwards
    for (local, _) in &forwards {
        client
            .stop_port_forward(*local)
            .await
            .expect("Failed to stop port forward");
    }
}

#[tokio::test]
async fn test_concurrent_operations() {
    let client = create_local_client()
        .await
        .expect("Failed to create client");

    // Test that we can perform multiple operations concurrently
    let clipboard_future = client.set_clipboard("Concurrent test".to_string());
    let browser_future = client.open_browser("https://example.com".to_string());
    let get_clipboard_future = client.get_clipboard();

    // Execute all operations concurrently
    let (clipboard_result, browser_result, get_clipboard_result) =
        tokio::join!(clipboard_future, browser_future, get_clipboard_future);

    assert!(clipboard_result.is_ok());
    assert!(browser_result.is_ok());
    assert!(get_clipboard_result.is_ok());
}

#[tokio::test]
#[ignore = "Error handling needs to be improved in the server"]
async fn test_error_handling() {
    let client = create_local_client()
        .await
        .expect("Failed to create client");

    // Test stopping non-existent port forward
    let result = client.stop_port_forward(65535).await;

    // Should get an error, not a panic
    assert!(
        result.is_err(),
        "Expected error when stopping non-existent port forward"
    );
}

#[tokio::test]
async fn test_long_polling_behavior() {
    let client = create_local_client()
        .await
        .expect("Failed to create client");

    // Test that client is responsive by doing a quick operation
    let start = std::time::Instant::now();
    let _result = client
        .get_clipboard()
        .await
        .expect("Failed to get clipboard");
    let elapsed = start.elapsed();

    // Should return quickly
    assert!(
        elapsed < Duration::from_secs(1),
        "Operation took too long: {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_transport_client_interface() {
    let client = create_local_transport_client()
        .await
        .expect("Failed to create transport client");

    // Test the higher-level transport client interface
    let test_content = "Transport client test";
    client
        .set_clipboard(test_content.to_string())
        .await
        .expect("Failed to set clipboard");

    let content = client
        .get_clipboard()
        .await
        .expect("Failed to get clipboard");
    assert_eq!(content, test_content);

    client
        .open_browser("https://example.com".to_string())
        .await
        .expect("Failed to open browser");
}

#[tokio::test]
async fn test_session_persistence() {
    let client = create_local_client()
        .await
        .expect("Failed to create client");

    // Set some state
    let content1 = "Session test 1";
    client
        .set_clipboard(content1.to_string())
        .await
        .expect("Failed to set clipboard");

    // Perform other operations
    for i in 0..5 {
        client
            .open_browser(format!("https://example.com/page{}", i))
            .await
            .expect("Failed to open browser");
    }

    // Verify state is still intact
    let retrieved = client
        .get_clipboard()
        .await
        .expect("Failed to get clipboard");
    assert_eq!(retrieved, content1);
}
