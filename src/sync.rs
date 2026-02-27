use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SYNC_CONFIG_FILE: &str = ".curlish-sync.toml";
const SYNC_REPO_DIR: &str = ".curlish-repo";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub repo_url: String,
    #[serde(default = "default_branch")]
    pub branch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    Disabled,
    Ok,
    Conflict,
}

fn default_branch() -> String {
    String::from("main")
}

pub fn config_path() -> PathBuf {
    PathBuf::from(SYNC_CONFIG_FILE)
}

pub fn load_config() -> Option<SyncConfig> {
    crate::config::load(&config_path())
}

pub fn save_config(config: &SyncConfig) -> Result<()> {
    crate::config::save(&config_path(), config)
}

/// Set `HOME` and `GIT_SSH_COMMAND` so child git/ssh processes can locate
/// SSH keys on Windows.  Call once, early, on the main thread.
pub fn setup_git_ssh_env() {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();

    if home.is_empty() {
        return;
    }

    // SAFETY: called once, single-threaded, before any other work.
    unsafe { std::env::set_var("HOME", &home); }

    if std::env::var("GIT_SSH_COMMAND").is_ok() || std::env::var("GIT_SSH").is_ok() {
        return;
    }

    if let Some(key) = find_ssh_key(&PathBuf::from(&home).join(".ssh")) {
        let path = key.to_string_lossy().replace('\\', "/");
        unsafe {
            std::env::set_var(
                "GIT_SSH_COMMAND",
                format!("ssh -i \"{path}\" -o IdentitiesOnly=yes -o StrictHostKeyChecking=accept-new"),
            );
        }
    }
}

/// Return the first private-key file found in `dir`, preferring modern key types.
fn find_ssh_key(dir: &Path) -> Option<PathBuf> {
    for name in ["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"] {
        let key = dir.join(name);
        if key.is_file() {
            return Some(key);
        }
    }
    // Fallback: any `id_*` file that isn't a .pub
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        let n = p.file_name()?.to_string_lossy().to_string();
        if n.starts_with("id_") && !n.ends_with(".pub") && p.is_file() {
            return Some(p);
        }
    }
    None
}

fn git_cmd() -> Command {
    let mut cmd = Command::new("git");
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", &home);
    } else if let Ok(profile) = std::env::var("USERPROFILE") {
        cmd.env("HOME", &profile);
    }
    cmd
}


fn repo_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(SYNC_REPO_DIR)
}


pub fn is_git_available() -> bool {
    git_cmd()
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── Git helper functions ────────────────────────────────────────────

fn has_local_changes(dir: &Path) -> Result<bool> {
    let output = git_cmd()
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output()
        .context("Failed to run git status")?;
    Ok(!output.stdout.is_empty())
}

fn auto_commit(dir: &Path) -> Result<()> {
    let _ = git_cmd().args(["add", "."]).current_dir(dir).output();
    let _ = git_cmd()
        .args(["commit", "-m", "curlish sync"])
        .current_dir(dir)
        .output();
    Ok(())
}

/// Full bidirectional sync: commit local changes, fetch, rebase, push.
/// Returns Ok(true) on success, Ok(false) on conflict needing manual intervention.
fn git_sync(dir: &Path, cfg: &SyncConfig) -> Result<Option<SyncStatus>> {
    // Stage & commit any local changes
    if has_local_changes(dir)? {
        auto_commit(dir)?;
    }

    // Fetch remote
    let fetch = git_cmd()
        .args(["fetch", "origin", &cfg.branch])
        .current_dir(dir)
        .output()
        .context("Failed to run git fetch")?;

    if !fetch.status.success() {
        let stderr = String::from_utf8_lossy(&fetch.stderr);
        // Remote branch doesn't exist yet — not an error
        if stderr.contains("couldn't find remote ref") {
            return Ok(None); // signal: no remote branch yet
        }
        bail!("Fetch failed: {}", stderr.trim());
    }

    // Rebase onto remote
    let remote_ref = format!("origin/{}", cfg.branch);
    let rebase = git_cmd()
        .args(["rebase", &remote_ref])
        .current_dir(dir)
        .output()
        .context("Failed to run git rebase")?;

    if !rebase.status.success() {
        // Abort the failed rebase
        let _ = git_cmd().args(["rebase", "--abort"]).current_dir(dir).output();
        return Ok(Some(SyncStatus::Conflict));
    }

    // Push
    let push = git_cmd()
        .args(["push", "origin", &cfg.branch])
        .current_dir(dir)
        .output()
        .context("Failed to run git push")?;

    if !push.status.success() {
        let stderr = String::from_utf8_lossy(&push.stderr);
        bail!("Push failed: {}", stderr.trim());
    }

    Ok(Some(SyncStatus::Ok))
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

#[allow(dead_code)]
pub fn pull(cfg: &SyncConfig, storage_path: &Path) -> Result<SyncStatus> {
    let dir = match ensure_repo(cfg) {
        Ok(d) => d,
        Err(_) => return Ok(SyncStatus::Disabled),
    };

    match git_sync(&dir, cfg)? {
        Some(SyncStatus::Ok) | None => {
            copy_from_repo(&dir, storage_path);
            Ok(SyncStatus::Ok)
        }
        Some(status) => Ok(status),
    }
}

pub fn push(cfg: &SyncConfig, storage_path: &Path) -> Result<SyncStatus> {
    let dir = match ensure_repo(cfg) {
        Ok(d) => d,
        Err(_) => return Ok(SyncStatus::Disabled),
    };

    // Detect divergence: if the repo already has a version of the file
    // that differs from local, treat it as a conflict rather than
    // silently overwriting the remote.
    let repo_file = dir.join(storage_path.file_name().unwrap_or_default());
    if repo_file.exists() && storage_path.exists() {
        let local = fs::read(storage_path).unwrap_or_default();
        let remote = fs::read(&repo_file).unwrap_or_default();
        if local != remote {
            return Ok(SyncStatus::Conflict);
        }
    }

    copy_to_repo(&dir, storage_path);

    match git_sync(&dir, cfg)? {
        Some(status) => Ok(status),
        None => {
            // No remote branch yet — bootstrap
            bootstrap_push(&dir, cfg)?;
            Ok(SyncStatus::Ok)
        }
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

    if has_local_changes(&dir)? {
        auto_commit(&dir)?;
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
