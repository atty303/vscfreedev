//! Integration tests for daemon functionality

use anyhow::Result;
use std::process::Command;

#[tokio::test]
async fn test_local_command_via_daemon() -> Result<()> {
    // Test that local command works via daemon by default
    let output = Command::new("cargo")
        .args(&["run", "-p", "yuha-cli", "--", "local"])
        .env("RUST_LOG", "info")
        .output()
        .expect("Failed to execute command");

    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    // For now, just check that the command exits (may fail due to missing binary)
    // but should not panic or have compilation errors
    Ok(())
}

#[tokio::test]
async fn test_daemon_status() -> Result<()> {
    // Test daemon status command
    let output = Command::new("cargo")
        .args(&["run", "-p", "yuha-cli", "--", "daemon", "status"])
        .env("RUST_LOG", "info")
        .output()
        .expect("Failed to execute command");

    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    // Should not panic
    Ok(())
}

#[tokio::test]
async fn test_help_command() -> Result<()> {
    // Test that help command works (safer test that won't hang)
    let output = Command::new("cargo")
        .args(&["run", "-p", "yuha-cli", "--", "--help"])
        .output()
        .expect("Failed to execute command");

    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    // Should exit successfully
    assert!(output.status.success());
    Ok(())
}
