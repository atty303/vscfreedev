pub(crate) mod docker;

use std::net::TcpListener;

/// Get a random available port for testing
pub fn get_random_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to random port")
        .local_addr()
        .expect("Failed to get local address")
        .port()
}
