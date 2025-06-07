# vscfreedev E2E Tests

This crate contains end-to-end tests for the vscfreedev project. The tests use Docker to create a remote environment and test the CLI's ability to connect to it via SSH.

## Prerequisites

- Docker must be installed and running
- Rust and Cargo must be installed

## Running the Tests

To run the E2E tests, use the following command from the project root:

```bash
cargo test -p vscfreedev_e2e
```

## How It Works

The E2E tests:

1. Build the remote server binary
2. Create a Docker container with:
   - An SSH server
   - The remote server running on port 9999
3. Run the CLI with SSH connection parameters pointing to the Docker container
4. Verify that the CLI can successfully connect and exchange messages

## Test Files

- `docker.rs`: Contains code for managing Docker containers
- `tests/cli_test.rs`: Contains the actual E2E tests

## Adding New Tests

To add new E2E tests:

1. Add a new test function in `tests/cli_test.rs` or create a new test file
2. Use the `RemoteContainer` struct to create a Docker container
3. Use `assert_cmd::Command` to run the CLI and make assertions about its output