# Testing Guide for Yuha Client

This document describes the testing structure and how to run different types of tests.

## Test Organization

The client tests are organized into functional units:

### Unit Tests (`tests/unit/`)
Fast tests that don't require external dependencies:
- `local_communication.rs` - Local process communication and clipboard/browser functionality
- `protocol_serialization.rs` - Protocol message serialization/deserialization
- `local_transport.rs` - Local transport implementation
- `comprehensive_local.rs` - Comprehensive local functionality tests

### Docker Tests (`tests/docker/`)
Integration tests that require Docker containers:
- `ssh_communication.rs` - SSH communication with auto-upload
- `port_forwarding.rs` - Port forwarding through SSH
- `binary_transfer.rs` - Binary upload and execution tests

## Running Tests

### Unit Tests Only (Fast)
Run only the fast unit tests without Docker dependencies:
```bash
cargo test -p yuha-client
```

### All Tests Including Docker
Run all tests including Docker-based integration tests:
```bash
cargo test -p yuha-client --features docker-tests
```

### Specific Test Categories
Run specific test modules:
```bash
# Unit tests only
cargo test -p yuha-client unit::

# Docker tests only (requires Docker)
cargo test -p yuha-client --features docker-tests docker::

# Specific test file
cargo test -p yuha-client --features docker-tests docker::ssh_communication::
```

### Environment Variables
Control test execution with environment variables:

```bash
# Skip Docker tests entirely
YUHA_SKIP_DOCKER=1 cargo test -p yuha-client

# Run only fast tests (skips Docker)
YUHA_FAST_TEST=1 cargo test -p yuha-client

# Debug logging
RUST_LOG=debug cargo test -p yuha-client
```

## Test Dependencies

### Unit Tests
- No external dependencies
- Use local process execution
- Fast execution (< 10 seconds total)

### Docker Tests
- Require Docker daemon running
- Create temporary SSH containers
- Slower execution (30-60 seconds total)
- Use `docker_test!()` macro for conditional execution

## Test Structure

Each test category is designed to test specific functionality:

1. **Protocol Tests** - Verify message serialization works correctly
2. **Transport Tests** - Test different connection methods (local, SSH)
3. **Communication Tests** - End-to-end functionality (clipboard, browser, port forwarding)
4. **Integration Tests** - Full workflow tests with Docker containers

## Debugging Tests

### View Test Output
```bash
# Show all test output
cargo test -p yuha-client -- --nocapture

# Show specific failed test
cargo test -p yuha-client test_name -- --nocapture
```

### Docker Test Debugging
Docker tests include container log output on failure. If a test fails:
1. Check the printed container logs
2. Verify Docker daemon is running
3. Check available ports for conflicts

### Common Issues
- **Port conflicts**: Tests use random ports, but conflicts can occur
- **Docker unavailable**: Set `YUHA_SKIP_DOCKER=1` to skip Docker tests
- **Timing issues**: Some tests have timing dependencies - rerun if flaky

## Adding New Tests

### For Unit Tests
Add to appropriate module in `tests/unit/` directory.

### For Docker Tests
1. Add to appropriate module in `tests/docker/` directory
2. Use `#[cfg(feature = "docker-tests")]` attribute
3. Use `docker_test!()` macro at test start
4. Include container log output on errors

### Test Conventions
- Use `#[serial]` for tests that can't run in parallel
- Use descriptive test names with functionality being tested
- Include timing information for performance-sensitive tests
- Clean up resources (containers auto-cleanup on drop)