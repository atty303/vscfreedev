//! Shared utilities for transport implementations

use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::warn;

/// Generic process stream adapter that wraps child process stdio
pub struct ProcessStream {
    child: Option<Child>,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl ProcessStream {
    /// Create a new process stream from child process components
    pub fn new(child: Child, stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            child: Some(child),
            stdin,
            stdout,
        }
    }
}

impl Drop for ProcessStream {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if let Err(e) = child.start_kill() {
                warn!("Failed to kill child process: {}", e);
            }
        }
    }
}

impl AsyncRead for ProcessStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdout).poll_read(cx, buf)
    }
}

impl AsyncWrite for ProcessStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}

/// Configure common command options
pub fn configure_command(
    cmd: &mut Command,
    env_vars: &std::collections::HashMap<String, String>,
    working_dir: &Option<std::path::PathBuf>,
) {
    // Set environment variables
    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    // Set working directory if specified
    if let Some(working_dir) = working_dir {
        cmd.current_dir(working_dir);
    }
}

/// Spawn a task to handle stderr logging
pub fn spawn_stderr_logger(stderr: tokio::process::ChildStderr, prefix: &str) {
    let prefix = prefix.to_string();
    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let reader = tokio::io::BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!("{} stderr: {}", prefix, line);
        }
    });
}
