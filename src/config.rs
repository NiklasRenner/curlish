use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::path::Path;

pub fn load<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let contents = fs::read_to_string(path).ok()?;
    toml::from_str(&contents).ok()
}

pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let contents = toml::to_string_pretty(value)?;
    fs::write(path, contents).context("Failed to write config file")?;
    Ok(())
}
