use anyhow::{bail, Context, Result};
use git_sync_rs::{RepositorySynchronizer, SyncConfig as GitSyncConfig, SyncError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SYNC_CONFIG_FILE: &str = ".curlish-sync.toml";
const SYNC_REPO_DIR: &str = ".curlish-repo";

fn git_cmd() -> Command {
    let mut cmd = Command::new("git");
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", &home);
    } else if let Ok(profile) = std::env::var("USERPROFILE") {
        cmd.env("HOME", &profile);
    }
    cmd
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub repo_url: String,
    #[serde(default = "default_branch")]
    pub branch: String,
}

fn default_branch() -> String {
    String::from("main")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    Disabled,
    Ok,
    Conflict,
}

pub fn config_path() -> PathBuf {
    PathBuf::from(SYNC_CONFIG_FILE)
}

fn repo_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(SYNC_REPO_DIR)
}

pub fn load_config() -> Option<SyncConfig> {
    let path = config_path();
    let contents = fs::read_to_string(&path).ok()?;
    toml::from_str(&contents).ok()
}

pub fn save_config(config: &SyncConfig) -> Result<()> {
    let contents = toml::to_string_pretty(config)?;
    fs::write(config_path(), contents).context("Failed to write sync config")?;
    Ok(())
}

pub fn is_git_available() -> bool {
    git_cmd()
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn make_sync_config(cfg: &SyncConfig) -> GitSyncConfig {
    GitSyncConfig {
        sync_new_files: true,
        skip_hooks: true,
        commit_message: Some("curlish sync".to_string()),
        remote_name: "origin".to_string(),
        branch_name: cfg.branch.clone(),
        conflict_branch: false,
        target_branch: None,
    }
}

// ── Repo bootstrap ──────────────────────────────────────────────────

fn ensure_repo(cfg: &SyncConfig) -> Result<PathBuf> {
    let dir = repo_dir();

    if !dir.join(".git").exists() {
        let dir_str = dir.to_str().unwrap_or(SYNC_REPO_DIR);
        let output = git_cmd()
            .args(["clone", &cfg.repo_url, dir_str])
            .output()
            .context("Failed to run git clone")?;

        if !output.status.success() && !dir.join(".git").exists() {
            init_local_repo(&dir, cfg)?;
        }
    }

    ensure_git_user(&dir);

    let _ = git_cmd()
        .args(["remote", "set-url", "origin", &cfg.repo_url])
        .current_dir(&dir)
        .output();

    Ok(dir)
}

fn init_local_repo(dir: &Path, cfg: &SyncConfig) -> Result<()> {
    fs::create_dir_all(dir).context("Failed to create sync repo directory")?;
    let output = git_cmd()
        .args(["init"])
        .current_dir(dir)
        .output()
        .context("Failed to run git init")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git init failed: {}", stderr.trim());
    }
    let _ = git_cmd().args(["remote", "add", "origin", &cfg.repo_url]).current_dir(dir).output();
    let _ = git_cmd().args(["checkout", "-b", &cfg.branch]).current_dir(dir).output();
    Ok(())
}

fn ensure_git_user(dir: &Path) {
    let needs_name = !git_cmd().args(["config", "user.name"]).current_dir(dir)
        .output().map(|o| o.status.success()).unwrap_or(false);
    let needs_email = !git_cmd().args(["config", "user.email"]).current_dir(dir)
        .output().map(|o| o.status.success()).unwrap_or(false);

    if needs_name {
        let _ = git_cmd().args(["config", "user.name", "curlish"]).current_dir(dir).output();
    }
    if needs_email {
        let _ = git_cmd().args(["config", "user.email", "curlish@localhost"]).current_dir(dir).output();
    }
}

// ── File copying ────────────────────────────────────────────────────

fn copy_from_repo(repo_dir: &Path, storage_path: &Path) {
    let repo_file = repo_dir.join(storage_path.file_name().unwrap_or_default());
    if repo_file.exists() {
        let _ = fs::copy(&repo_file, storage_path);
    }
}

fn copy_to_repo(repo_dir: &Path, storage_path: &Path) {
    let repo_file = repo_dir.join(storage_path.file_name().unwrap_or_default());
    if storage_path.exists() {
        let _ = fs::copy(storage_path, &repo_file);
    }
}

// ── Public sync operations ──────────────────────────────────────────

pub fn init(cfg: &SyncConfig, storage_path: &Path) -> Result<()> {
    let dir = repo_dir();
    if dir.exists() && !dir.join(".git").exists() {
        let _ = fs::remove_dir_all(&dir);
    }
    let d = ensure_repo(cfg)?;
    copy_from_repo(&d, storage_path);
    Ok(())
}

pub fn pull(cfg: &SyncConfig, storage_path: &Path) -> Result<SyncStatus> {
    let dir = match ensure_repo(cfg) {
        Ok(d) => d,
        Err(_) => return Ok(SyncStatus::Disabled),
    };

    let mut syncer = match RepositorySynchronizer::new(&dir, make_sync_config(cfg)) {
        Ok(s) => s,
        Err(_) => return Ok(SyncStatus::Disabled),
    };

    match syncer.sync(false) {
        Ok(()) => {
            copy_from_repo(&dir, storage_path);
            Ok(SyncStatus::Ok)
        }
        Err(SyncError::ManualInterventionRequired { .. }) => Ok(SyncStatus::Conflict),
        Err(SyncError::RemoteBranchNotFound { .. })
        | Err(SyncError::NoRemoteConfigured { .. })
        | Err(SyncError::GitError(_)) => Ok(SyncStatus::Ok),
        Err(e) => bail!("Pull failed: {e}"),
    }
}

pub fn push(cfg: &SyncConfig, storage_path: &Path) -> Result<SyncStatus> {
    let dir = match ensure_repo(cfg) {
        Ok(d) => d,
        Err(_) => return Ok(SyncStatus::Disabled),
    };

    copy_to_repo(&dir, storage_path);

    let mut syncer = match RepositorySynchronizer::new(&dir, make_sync_config(cfg)) {
        Ok(s) => s,
        Err(_) => return Ok(SyncStatus::Disabled),
    };

    match syncer.sync(false) {
        Ok(()) => Ok(SyncStatus::Ok),
        Err(SyncError::ManualInterventionRequired { .. }) => Ok(SyncStatus::Conflict),
        Err(SyncError::RemoteBranchNotFound { .. })
        | Err(SyncError::NoRemoteConfigured { .. })
        | Err(SyncError::GitError(_)) => {
            bootstrap_push(&dir, cfg)?;
            Ok(SyncStatus::Ok)
        }
        Err(e) => bail!("Push failed: {e}"),
    }
}

fn bootstrap_push(dir: &Path, cfg: &SyncConfig) -> Result<()> {
    let _ = git_cmd().args(["add", "."]).current_dir(dir).output();
    let _ = git_cmd().args(["commit", "-m", "curlish: initial sync"]).current_dir(dir).output();

    let output = git_cmd()
        .args(["push", "-u", "origin", &cfg.branch])
        .current_dir(dir)
        .output()
        .context("Failed to push")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Initial push failed: {}", stderr.trim());
    }
    Ok(())
}

pub fn force_push(cfg: &SyncConfig, storage_path: &Path) -> Result<()> {
    let dir = ensure_repo(cfg)?;
    copy_to_repo(&dir, storage_path);

    let syncer = RepositorySynchronizer::new(&dir, make_sync_config(cfg))?;
    if syncer.has_local_changes()? {
        syncer.auto_commit()?;
    }

    let output = git_cmd()
        .args(["push", "--force", "origin", &cfg.branch])
        .current_dir(&dir)
        .output()
        .context("Failed to run git push --force")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Force push failed: {}", stderr.trim());
    }
    Ok(())
}

pub fn force_pull(cfg: &SyncConfig, storage_path: &Path) -> Result<()> {
    let dir = ensure_repo(cfg)?;

    let output = git_cmd()
        .args(["fetch", "origin", &cfg.branch])
        .current_dir(&dir)
        .output()
        .context("Failed to fetch")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Fetch failed: {}", stderr.trim());
    }

    let remote_ref = format!("origin/{}", cfg.branch);
    let output = git_cmd()
        .args(["reset", "--hard", &remote_ref])
        .current_dir(&dir)
        .output()
        .context("Failed to reset")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Reset failed: {}", stderr.trim());
    }

    copy_from_repo(&dir, storage_path);
    Ok(())
}
