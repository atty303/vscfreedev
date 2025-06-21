// This file has been refactored and split into:
// - unit/local_communication.rs - for local process tests
// - docker/ssh_communication.rs - for Docker-based SSH tests

mod docker;
mod shared;
mod unit;
