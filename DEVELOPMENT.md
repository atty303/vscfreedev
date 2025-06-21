# Development Guide

This guide provides comprehensive instructions for setting up the development environment and contributing to the Yuha project.

## Prerequisites

- **Rust**: Version 1.70+ with `cargo` and `rustc`
- **Docker**: Required for integration tests (optional for basic development)
- **SSH Access**: For manual testing of SSH transport (optional)

### Platform-Specific Requirements

#### Linux/macOS
- Standard development tools (`build-essential` on Ubuntu, Xcode Command Line Tools on macOS)
- `pkg-config` for native dependencies

#### Windows
- Visual Studio Build Tools or Visual Studio Community
- Windows Subsystem for Linux (WSL) support (optional, for WSL transport testing)

## Quick Start

1. **Clone the repository**:
   ```bash
   git clone <repository-url>
   cd yuha
   ```

2. **Verify installation**:
   ```bash
   cargo --version
   rustc --version
   ```

3. **Build the project**:
   ```bash
   cargo build
   ```

4. **Run fast tests** (no external dependencies):
   ```bash
   cargo test
   ```

5. **Run all tests** (requires Docker):
   ```bash
   cargo test --features docker-tests
   ```

## Project Structure

```
yuha/
├── crates/
│   ├── core/           # Shared functionality (protocols, transports, utilities)
│   ├── client/         # Client-side implementation (CLI/GUI shared code)
│   ├── remote/         # Remote server implementation
│   ├── cli/            # Command-line interface
│   └── gui/            # Graphical user interface
├── tests/              # Integration tests
├── docs/               # Additional documentation
├── CLAUDE.md           # Project instructions for AI assistants
└── DEVELOPMENT.md      # This file
```

## Development Workflow

### Building Components

```bash
# Build all crates
cargo build

# Build specific crate
cargo build -p yuha-core
cargo build -p yuha-client
cargo build -p yuha-remote

# Build with optimizations (for performance testing)
cargo build --release
```

### Running Tests

The project uses a tiered testing approach for optimal development experience:

#### Fast Tests (Recommended for Development)
```bash
# Run all fast tests (no Docker required)
cargo test

# Run tests for specific crate
cargo test -p yuha-core
cargo test -p yuha-client
```

#### Integration Tests (Docker Required)
```bash
# Run all tests including Docker-based integration tests
cargo test --features docker-tests

# Run only Docker integration tests
cargo test --features docker-tests docker::
```

#### Test Categories

- **Unit Tests** (`tests/unit/`): Fast, no external dependencies
  - Protocol serialization/deserialization
  - Local transport and communication
  - Core functionality validation

- **Integration Tests** (`tests/docker/`): Slower, requires Docker
  - SSH communication with real containers
  - Port forwarding functionality
  - Binary upload and execution

### Running the Application

#### CLI Usage
```bash
# Run CLI directly
cargo run -p yuha-cli

# Run with arguments
cargo run -p yuha-cli -- --help

# Start daemon
cargo run -p yuha-cli -- daemon start

# Connect to remote server (example)
cargo run -p yuha-cli -- connect user@example.com
```

#### Remote Server
```bash
# Run remote server in stdio mode (for SSH)
cargo run -p yuha-remote -- --stdio

# Run with debug logging
RUST_LOG=debug cargo run -p yuha-remote -- --stdio
```

## Testing Strategy

### Writing Tests

1. **Prefer unit tests** for new functionality
2. **Use integration tests** for complex workflows
3. **Add Docker tests** only for features that require real SSH connections

### Test Organization

```rust
// Unit test example
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_functionality() {
        // Test implementation
    }
}

// Docker integration test example
#[cfg(feature = "docker-tests")]
mod docker_tests {
    use super::*;

    #[tokio::test]
    async fn test_ssh_integration() {
        // Docker-based test implementation
    }
}
```

## Debugging

### Logging

Enable detailed logging during development:

```bash
# Debug level logging
RUST_LOG=debug cargo run -p yuha-cli

# Trace level for maximum detail
RUST_LOG=trace cargo run -p yuha-remote -- --stdio

# Module-specific logging
RUST_LOG=yuha_client::transport::ssh=debug cargo run -p yuha-cli
```

### Remote Server Debugging

When developing SSH functionality, check these log files:

```bash
# Remote server startup logs
tail -f /tmp/remote_startup.txt

# Remote server error logs
tail -f /tmp/remote_stderr.log
```

### Transport Debugging

For connection issues:

```bash
# SSH transport debugging
RUST_LOG=yuha_client::transport::ssh=debug cargo run -p yuha-cli

# All transport debugging
RUST_LOG=yuha_client::transport=debug cargo run -p yuha-cli
```

## Common Development Tasks

### Adding a New Transport

1. Create transport module in `crates/client/src/transport/`
2. Implement `Transport` trait
3. Add configuration struct
4. Update `TransportFactory`
5. Add unit tests
6. Add integration tests if needed

### Adding a New Protocol Command

1. Add request variant to appropriate protocol enum
2. Add response handling
3. Implement client-side method
4. Implement server-side handling
5. Add tests for both client and server

### Modifying Protocol Messages

1. Update protocol definitions in `crates/core/src/protocol/`
2. Ensure backward compatibility or plan migration
3. Update serialization tests
4. Test with real client-server communication

## Code Style and Conventions

### Formatting

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check
```

### Linting

```bash
# Run clippy
cargo clippy

# Run clippy with all features
cargo clippy --features docker-tests

# Fix clippy suggestions (use with caution)
cargo clippy --fix
```

### Documentation

```bash
# Generate documentation
cargo doc

# Generate and open documentation
cargo doc --open

# Check documentation examples
cargo test --doc
```

## Performance Considerations

### Profiling

For performance analysis:

```bash
# Build with debug symbols
cargo build --profile dev-fast

# Use performance tools
perf record cargo run -p yuha-cli
```

### Benchmarking

```bash
# Run benchmarks (if available)
cargo bench

# Profile specific operations
cargo run --release -p yuha-cli -- benchmark
```

## Troubleshooting

### Common Issues

1. **Docker tests failing**:
   - Ensure Docker daemon is running
   - Check Docker permissions
   - Verify network connectivity

2. **SSH tests failing**:
   - Check SSH key permissions
   - Verify SSH server configuration
   - Check firewall settings

3. **Build failures**:
   - Update Rust toolchain: `rustup update`
   - Clean build cache: `cargo clean`
   - Check platform-specific dependencies

### Getting Help

1. Check existing issues in the project repository
2. Review the `CLAUDE.md` file for project-specific context
3. Run tests with `RUST_LOG=debug` for detailed error information
4. Use `cargo check` for quick syntax verification

## Contributing

### Before Submitting Changes

1. **Run all tests**:
   ```bash
   cargo test --features docker-tests
   ```

2. **Format and lint**:
   ```bash
   cargo fmt
   cargo clippy
   ```

3. **Update documentation** if needed:
   ```bash
   cargo doc
   ```

4. **Verify builds** on your platform:
   ```bash
   cargo build --release
   ```

### Commit Message Format

Follow conventional commit format:
- `feat:` for new features
- `fix:` for bug fixes  
- `docs:` for documentation changes
- `test:` for test additions/changes
- `refactor:` for code refactoring

Example: `feat: add WSL transport support for Windows`

This guide should provide everything needed to start contributing to the Yuha project effectively.