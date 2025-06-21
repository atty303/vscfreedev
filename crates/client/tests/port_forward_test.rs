// This file has been refactored and split into:
// - unit/local_communication.rs - for local port forwarding tests
// - docker/port_forwarding.rs - for Docker-based port forwarding tests

mod docker;
mod shared;
mod unit;
