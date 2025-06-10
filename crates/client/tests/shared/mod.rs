pub(crate) mod docker;
pub mod test_utils;

use std::net::TcpListener;
use std::time::Duration;

/// Get a random available port for testing
#[allow(dead_code)]
pub fn get_random_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to random port")
        .local_addr()
        .expect("Failed to get local address")
        .port()
}

/// Wait for a service to be ready on a specific port
#[allow(dead_code)]
pub async fn wait_for_service(host: &str, port: u16, timeout: Duration) -> anyhow::Result<()> {
    let start = std::time::Instant::now();

    loop {
        match tokio::net::TcpStream::connect(format!("{}:{}", host, port)).await {
            Ok(_) => return Ok(()),
            Err(_) => {
                if start.elapsed() > timeout {
                    anyhow::bail!(
                        "Service on {}:{} failed to start within {:?}",
                        host,
                        port,
                        timeout
                    );
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}
