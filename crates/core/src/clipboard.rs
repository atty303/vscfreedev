use crate::error::{ClipboardError, Result};
use std::sync::RwLock;
use tracing::debug;

static CLIPBOARD_STORAGE: RwLock<String> = RwLock::new(String::new());

/// Get clipboard content asynchronously
pub async fn get() -> Result<String> {
    get_clipboard()
}

/// Get clipboard content synchronously
pub fn get_clipboard() -> Result<String> {
    debug!("Attempting to read clipboard");

    let storage = CLIPBOARD_STORAGE
        .read()
        .map_err(|e| ClipboardError::LockFailed {
            reason: format!("Read lock poisoned: {}", e),
        })?;

    let content = storage.clone();
    debug!(
        "Successfully read clipboard content (length: {})",
        content.len()
    );
    Ok(content)
}

/// Set clipboard content
pub fn set_clipboard(content: &str) -> Result<()> {
    debug!(
        "Attempting to set clipboard content (length: {})",
        content.len()
    );

    let mut storage = CLIPBOARD_STORAGE
        .write()
        .map_err(|e| ClipboardError::LockFailed {
            reason: format!("Write lock poisoned: {}", e),
        })?;

    *storage = content.to_string();
    debug!("Successfully set clipboard content");
    Ok(())
}

/// Clear clipboard content
pub fn clear_clipboard() -> Result<()> {
    debug!("Clearing clipboard content");
    set_clipboard("")
}

/// Check if clipboard is empty
pub fn is_clipboard_empty() -> Result<bool> {
    let content = get_clipboard()?;
    Ok(content.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_operations() {
        // Test setting and getting content
        let test_content = "Hello, clipboard!";
        assert!(set_clipboard(test_content).is_ok());

        let retrieved = get_clipboard().unwrap();
        assert_eq!(retrieved, test_content);

        // Test clearing
        assert!(clear_clipboard().is_ok());
        assert!(is_clipboard_empty().unwrap());
    }

    #[tokio::test]
    async fn test_async_clipboard() {
        let test_content = "Async test";
        assert!(set_clipboard(test_content).is_ok());

        let retrieved = get().await.unwrap();
        assert_eq!(retrieved, test_content);
    }
}
