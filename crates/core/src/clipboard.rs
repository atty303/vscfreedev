use std::sync::RwLock;

static CLIPBOARD_STORAGE: RwLock<String> = RwLock::new(String::new());

pub async fn get() -> anyhow::Result<String> {
    let storage = CLIPBOARD_STORAGE.read().unwrap();
    Ok(storage.clone())
}

pub fn get_clipboard() -> anyhow::Result<String> {
    let storage = CLIPBOARD_STORAGE.read().unwrap();
    Ok(storage.clone())
}

pub fn set_clipboard(content: &str) -> anyhow::Result<()> {
    let mut storage = CLIPBOARD_STORAGE.write().unwrap();
    *storage = content.to_string();
    Ok(())
}
