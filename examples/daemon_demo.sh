#!/bin/bash
# Demo script for Yuha daemon functionality (integrated into CLI)

echo "=== Yuha Daemon Demo (Integrated) ==="
echo

# Check daemon status
echo "1. Checking daemon status..."
cargo run -p yuha-cli -- daemon status

# Start daemon in background
echo -e "\n2. Starting daemon..."
cargo run -p yuha-cli -- daemon start &
DAEMON_PID=$!

sleep 3

# Check status again
echo -e "\n3. Checking daemon status again..."
cargo run -p yuha-cli -- daemon status

# List sessions (should be empty)
echo -e "\n4. Listing sessions..."
cargo run -p yuha-cli -- daemon sessions

# Stop daemon
echo -e "\n5. Stopping daemon..."
cargo run -p yuha-cli -- daemon stop

sleep 2

# Check final status
echo -e "\n6. Final status check..."
cargo run -p yuha-cli -- daemon status

echo -e "\n=== Demo Complete ==="

# Note: In the integrated approach, the daemon runs as part of the CLI binary
echo "Note: The daemon is now integrated into the CLI binary and runs in the background"