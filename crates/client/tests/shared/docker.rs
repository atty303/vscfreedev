//! Docker container management for E2E tests

use anyhow::{Context, Result};
use rand::Rng;
use std::time::Duration;
use tokio::fs;
use tokio::process::Command;
use tokio::time::sleep;

/// Dockerfile content for the remote server (without pre-installed binary)
const DOCKERFILE: &str = r#"
FROM rust:slim

WORKDIR /app

# Install OpenSSH server, netcat, and other dependencies
RUN apt-get update && \
    apt-get install -y openssh-server netcat-openbsd socat && \
    mkdir -p /run/sshd && \
    echo 'root:password' | chpasswd && \
    sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    sed -i 's/#LogLevel INFO/LogLevel DEBUG/' /etc/ssh/sshd_config

# Expose SSH port and test service port
EXPOSE 22 8888

# Create a directory for logs
RUN mkdir -p /var/log/yuha

# Start SSH server and test echo service
RUN echo '#!/bin/bash\n/usr/sbin/sshd -E /var/log/sshd.log\necho "SSH server started"\n# Start a simple echo service on port 8888\nsocat TCP-LISTEN:8888,fork,reuseaddr EXEC:/bin/cat &\necho "Echo service started on port 8888"\ntail -f /var/log/sshd.log /tmp/remote*.log /tmp/yuha-remote-* 2>/dev/null || true' > /start.sh && \
    chmod +x /start.sh
CMD ["/start.sh"]
"#;

/// Container configuration for the remote server
pub struct RemoteContainer {
    container_name: String,
}

impl RemoteContainer {
    /// Create a new RemoteContainer
    pub async fn new() -> Result<Self> {
        // Generate a random container name
        let mut rng = rand::rng();
        let random_suffix: u32 = rng.random_range(10000..99999);
        let container_name = format!("yuha-remote-test-{}", random_suffix);

        // Create a temporary directory for the Docker build context
        let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;
        let dockerfile_path = temp_dir.path().join("Dockerfile");
        fs::write(&dockerfile_path, DOCKERFILE).await?;

        // Build the Docker image
        let image_name = format!("yuha-remote-test-{}", random_suffix);
        let build_status = Command::new("docker")
            .args([
                "build",
                "-t",
                &image_name,
                "-f",
                dockerfile_path.to_str().unwrap(),
                temp_dir.path().to_str().unwrap(),
            ])
            .status()
            .await
            .context("Failed to build Docker image")?;

        if !build_status.success() {
            anyhow::bail!("Docker build failed");
        }

        // Create and start the container
        let run_status = Command::new("docker")
            .args(["run", "-d", "--name", &container_name, "-P", &image_name])
            .status()
            .await
            .context("Failed to run Docker container")?;

        if !run_status.success() {
            anyhow::bail!("Docker run failed");
        }

        // Wait for SSH server to start
        let container = Self { container_name };
        container.wait_for_ssh().await?;
        Ok(container)
    }

    /// Get the logs from the container
    #[allow(dead_code)]
    pub async fn get_logs(&self) -> Result<String> {
        let output = Command::new("docker")
            .args(["logs", &self.container_name])
            .output()
            .await
            .context("Failed to get container logs")?;

        if !output.status.success() {
            anyhow::bail!("Docker logs command failed");
        }

        let logs = String::from_utf8(output.stdout).context("Failed to parse container logs")?;

        Ok(logs)
    }

    /// Get the SSH port for the container
    pub async fn ssh_port(&self) -> Result<u16> {
        self.get_mapped_port("22/tcp").await
    }

    /// Get the echo service port for the container
    #[allow(dead_code)]
    pub async fn echo_service_port(&self) -> Result<u16> {
        self.get_mapped_port("8888/tcp").await
    }

    /// Get a mapped port for the container
    async fn get_mapped_port(&self, port_spec: &str) -> Result<u16> {
        let output = Command::new("docker")
            .args(["port", &self.container_name, port_spec])
            .output()
            .await
            .context("Failed to get container port")?;

        if !output.status.success() {
            anyhow::bail!("Docker port command failed");
        }

        let port_mapping =
            String::from_utf8(output.stdout).context("Failed to parse port mapping")?;

        // Port mapping format is typically "0.0.0.0:49154"
        let port = port_mapping
            .trim()
            .split(':')
            .next_back()
            .context("Invalid port mapping format")?
            .parse::<u16>()
            .context("Failed to parse port number")?;

        Ok(port)
    }

    /// Wait for SSH server to be ready by attempting connection
    async fn wait_for_ssh(&self) -> Result<()> {
        let ssh_port = self.ssh_port().await?;
        let start_time = std::time::Instant::now();
        let max_wait = Duration::from_secs(10);

        loop {
            // Try to connect to SSH port
            match tokio::net::TcpStream::connect(format!("127.0.0.1:{}", ssh_port)).await {
                Ok(_) => {
                    // Connection successful, SSH is ready
                    println!(
                        "SSH server is ready on port {} (took {:.1}s)",
                        ssh_port,
                        start_time.elapsed().as_secs_f32()
                    );
                    return Ok(());
                }
                Err(_) => {
                    // Check if we've exceeded max wait time
                    if start_time.elapsed() > max_wait {
                        anyhow::bail!(
                            "SSH server failed to start within {} seconds",
                            max_wait.as_secs()
                        );
                    }
                    // Wait a short time before retrying
                    sleep(Duration::from_millis(50)).await;
                }
            }
        }
    }
}

impl Drop for RemoteContainer {
    fn drop(&mut self) {
        let container_name = self.container_name.clone();

        // Spawn a task to remove the container
        tokio::spawn(async move {
            let _ = Command::new("docker")
                .args(["rm", "-f", &container_name])
                .status()
                .await;
        });
    }
}
