//! Remote-side library for yuha

pub mod ipc;

/// Remote implementation
pub mod remote {
    /// Initialize the remote
    pub fn init() -> anyhow::Result<()> {
        Ok(())
    }
}
