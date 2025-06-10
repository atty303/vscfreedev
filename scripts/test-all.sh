#!/bin/bash
# Run all tests including slow Docker/SSH tests

echo "Running all tests (including slow tests)..."
export RUST_LOG=debug
unset YUHA_FAST_TEST

# Run tests with longer timeout for Docker tests
cargo test --workspace -- --nocapture

echo "All tests completed."