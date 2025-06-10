#!/bin/bash
# Run fast tests only (excluding Docker/SSH tests)

echo "Running fast tests only..."
export YUHA_FAST_TEST=1
export RUST_LOG=debug

# Run tests with timeout
cargo test --workspace -- --nocapture

echo "Fast tests completed."