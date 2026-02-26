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

pub fn has_unsaved_changes(path: &Path, store: &RequestStore) -> bool {
    let current = match serde_json::to_string_pretty(store) {
        Ok(s) => s,
        Err(_) => return true,
    };
    let on_disk = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return true, // file doesn't exist yet but we have data
    };
    current != on_disk
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HeaderEntry, HttpMethod, Request};
    use std::fs;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("curlish_test_{}_{}", std::process::id(), name));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_store() -> RequestStore {
        RequestStore {
            requests: vec![
                Request {
                    id: 1,
                    name: String::from("Test"),
                    method: HttpMethod::Post,
                    url: String::from("https://example.com/api"),
                    headers: vec![HeaderEntry {
                        name: String::from("Content-Type"),
                        value: String::from("application/json"),
                    }],
                    body: String::from(r#"{"key": "value"}"#),
                },
            ],
            environments: Vec::new(),
            active_environment: None,
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = test_dir("roundtrip");
        let path = dir.join("roundtrip.json");

        let store = test_store();
        save(&path, &store).unwrap();

        let loaded = load_or_default(&path).unwrap();
        assert_eq!(loaded.requests.len(), 1);
        assert_eq!(loaded.requests[0].name, "Test");
        assert_eq!(loaded.requests[0].method, HttpMethod::Post);
        assert_eq!(loaded.requests[0].url, "https://example.com/api");
        assert_eq!(loaded.requests[0].headers.len(), 1);
        assert_eq!(loaded.requests[0].body, r#"{"key": "value"}"#);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = test_dir("missing");
        let path = dir.join("nonexistent.json");

        let store = load_or_default(&path).unwrap();
        assert!(!store.requests.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_creates_valid_json() {
        let dir = test_dir("valid_json");
        let path = dir.join("valid.json");

        save(&path, &test_store()).unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(parsed.get("requests").unwrap().is_array());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_invalid_json_returns_error() {
        let dir = test_dir("invalid_json");
        let path = dir.join("bad.json");
        fs::write(&path, "not valid json {{{").unwrap();

        let result = load_or_default(&path);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&dir);
    }
}
