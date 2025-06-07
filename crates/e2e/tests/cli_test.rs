use anyhow::Result;
use assert_cmd::Command;
use std::time::Duration;
use tokio::time::sleep;
use vscfreedev_e2e::docker::RemoteContainer;

#[tokio::test]
async fn test_ssh_connection() -> Result<()> {
    // Start the Docker container with the remote server
    let container = RemoteContainer::new().await?;
    let ssh_port = container.ssh_port().await?;

    // Wait a bit for the SSH server to be fully ready
    sleep(Duration::from_secs(3)).await;

    // Run the CLI with an SSH connection
    let mut cmd = Command::new("cargo");
    cmd.args(&["run", "-p", "vscfreedev-cli", "--"]).arg("ssh")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(ssh_port.to_string())
        .arg("--username")
        .arg("root")
        .arg("--password")
        .arg("password")
        .arg("--message")
        .arg("Hello from E2E test!");

    // Execute the command and check the output
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("Connecting to 127.0.0.1"))
        .stdout(predicates::str::contains("Connected to remote host"))
        .stdout(predicates::str::contains("Remote channel established"))
        .stdout(predicates::str::contains(
            "Sending message: Hello from E2E test!",
        ))
        .stdout(predicates::str::contains("Echo: Hello from E2E test!"))
        .stdout(predicates::str::contains(
            "SSH connection test completed successfully",
        ));

    Ok(())
}
