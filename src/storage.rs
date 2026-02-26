use crate::model::RequestStore;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn default_path() -> PathBuf {
    PathBuf::from(".curlish.json")
}

pub fn load_or_default(path: &Path) -> Result<RequestStore> {
    if !path.exists() {
        return Ok(RequestStore::default());
    }

    let contents = fs::read_to_string(path).with_context(|| {
        format!("Failed to read request store from {}", path.display())
    })?;
    let store = serde_json::from_str(&contents).with_context(|| {
        format!("Failed to parse request store from {}", path.display())
    })?;
    Ok(store)
}

pub fn save(path: &Path, store: &RequestStore) -> Result<()> {
    let contents = serde_json::to_string_pretty(store)?;
    fs::write(path, contents)
        .with_context(|| format!("Failed to write request store to {}", path.display()))?;
    Ok(())
}

