//! Integration tests for sync.rs
//!
//! Each test creates temporary "remote" (bare) and "local" git repos
//! via `SyncConfig::local_dir`, so tests are fully isolated and can
//! run in parallel.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

// ── Re-export the crate under test ──────────────────────────────────

use curlish::sync::{self, SyncConfig, SyncStatus};

// ── Helpers ─────────────────────────────────────────────────────────

struct TestEnv {
    _root: TempDir,
    remote: PathBuf,
    local: PathBuf,
    storage: PathBuf,
}

impl TestEnv {
    /// Create a bare "remote" repo and point CURLISH_REPO_DIR at a
    /// `local` subdirectory (which does NOT yet have a `.git`).
    fn new() -> Self {
        let root = TempDir::new().expect("create temp dir");
        let remote = root.path().join("remote.git");
        let local = root.path().join("local");
        let storage = root.path().join("requests.json");

        // Create bare remote
        git(&remote, &["init", "--bare"]);

        Self { _root: root, remote, local, storage }
    }

    fn cfg(&self) -> SyncConfig {
        SyncConfig {
            repo_url: self.remote.to_string_lossy().into_owned(),
            branch: "main".into(),
            local_dir: self.local.to_string_lossy().into_owned(),
        }
    }

    /// Write some JSON content to the local storage file.
    fn write_storage(&self, content: &str) {
        fs::write(&self.storage, content).expect("write storage");
    }

    /// Read the local storage file.
    fn read_storage(&self) -> String {
        fs::read_to_string(&self.storage).unwrap_or_default()
    }

    /// Push a file directly to the bare remote (simulating another client).
    /// Creates a temporary clone, commits the file, and pushes.
    fn push_to_remote(&self, filename: &str, content: &str) {
        let clone_dir = self._root.path().join("other_client");
        if clone_dir.exists() {
            fs::remove_dir_all(&clone_dir).ok();
        }
        git_at(
            self._root.path(),
            &["clone", &self.remote.to_string_lossy(), "other_client"],
        );
        // Ensure branch exists
        let _ = Command::new("git")
            .args(["checkout", "-b", "main"])
            .current_dir(&clone_dir)
            .output();
        fs::write(clone_dir.join(filename), content).expect("write file in clone");
        git(&clone_dir, &["add", "."]);
        git(&clone_dir, &["commit", "-m", "remote commit"]);
        git(&clone_dir, &["push", "-u", "origin", "main"]);
    }
}

fn git(dir: &Path, args: &[&str]) {
    if !dir.exists() {
        fs::create_dir_all(dir).expect("create dir for git");
    }
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("git {args:?} in {} failed: {stderr}", dir.display());
    }
}

fn git_at(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("git {args:?} in {} failed: {stderr}", dir.display());
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[test]
fn push_to_empty_remote() {
    // Fresh local repo, empty remote → should push without conflict.
    let env = TestEnv::new();
    env.write_storage(r#"{"requests":[]}"#);

    let status = sync::push(&env.cfg(), &env.storage).expect("push");
    assert_eq!(status, SyncStatus::Ok);
}

#[test]
fn push_local_ahead() {
    // Set up: remote has one commit, local repo is in sync.
    // Then write new local content and push → should succeed (local ahead).
    let env = TestEnv::new();
    env.push_to_remote("requests.json", r#"{"requests":["v1"]}"#);

    // First sync: fresh repo + remote has content → conflict.
    // Resolve by force-pulling to establish shared history.
    env.write_storage("");
    let status = sync::push(&env.cfg(), &env.storage).expect("initial push");
    assert_eq!(status, SyncStatus::Conflict);
    sync::force_pull(&env.cfg(), &env.storage).expect("force pull");
    assert_eq!(env.read_storage(), r#"{"requests":["v1"]}"#);

    // Now write a new version and push — local is ahead.
    env.write_storage(r#"{"requests":["v2"]}"#);
    let status = sync::push(&env.cfg(), &env.storage).expect("push v2");
    assert_eq!(status, SyncStatus::Ok);
}

#[test]
fn push_remote_ahead_auto_pulls() {
    // Local repo is in sync with remote. Another client pushes.
    // Our push should auto-pull (remote strictly ahead).
    let env = TestEnv::new();

    // Bootstrap: push initial content.
    env.write_storage(r#"{"requests":["v1"]}"#);
    let status = sync::push(&env.cfg(), &env.storage).expect("initial push");
    assert_eq!(status, SyncStatus::Ok);

    // Simulate another client pushing v2.
    env.push_to_remote("requests.json", r#"{"requests":["v2"]}"#);

    // Our push should detect remote is ahead and auto-pull.
    let status = sync::push(&env.cfg(), &env.storage).expect("push after remote update");
    assert_eq!(status, SyncStatus::Ok);
    assert_eq!(env.read_storage(), r#"{"requests":["v2"]}"#);
}

#[test]
fn push_diverged_returns_conflict() {
    // Both local and remote have unique commits → conflict.
    let env = TestEnv::new();

    // Bootstrap shared history.
    env.write_storage(r#"{"requests":["v1"]}"#);
    sync::push(&env.cfg(), &env.storage).expect("initial push");

    // Simulate remote advancing.
    env.push_to_remote("requests.json", r#"{"requests":["v2-remote"]}"#);

    // Advance local too (commit into local repo).
    env.write_storage(r#"{"requests":["v2-local"]}"#);
    fs::copy(&env.storage, env.local.join("requests.json")).ok();
    git(&env.local, &["add", "."]);
    git(&env.local, &["commit", "-m", "local v2"]);

    // Now push — should be diverged.
    let status = sync::push(&env.cfg(), &env.storage).expect("push diverged");
    assert_eq!(status, SyncStatus::Conflict);
}

#[test]
fn fresh_repo_remote_has_content_returns_conflict() {
    // No local repo. Remote already has commits. Should ask user.
    let env = TestEnv::new();
    env.push_to_remote("requests.json", r#"{"requests":["remote"]}"#);

    env.write_storage(r#"{"requests":["local"]}"#);
    let status = sync::push(&env.cfg(), &env.storage).expect("push");
    assert_eq!(status, SyncStatus::Conflict);
}

#[test]
fn force_push_overwrites_remote() {
    let env = TestEnv::new();
    env.push_to_remote("requests.json", r#"{"requests":["remote"]}"#);

    // Force pull first to bootstrap local repo with remote history.
    env.write_storage("");
    let _ = sync::push(&env.cfg(), &env.storage); // triggers ensure_repo
    sync::force_pull(&env.cfg(), &env.storage).expect("force pull");

    // Now force push local content.
    env.write_storage(r#"{"requests":["forced"]}"#);
    sync::force_push(&env.cfg(), &env.storage).expect("force push");

    // Verify: clone fresh and check content.
    let verify = env._root.path().join("verify");
    git_at(env._root.path(), &["clone", &env.remote.to_string_lossy(), "verify"]);
    let content = fs::read_to_string(verify.join("requests.json")).expect("read");
    assert_eq!(content, r#"{"requests":["forced"]}"#);
}

#[test]
fn force_pull_overwrites_local() {
    let env = TestEnv::new();
    env.push_to_remote("requests.json", r#"{"requests":["remote"]}"#);

    env.write_storage(r#"{"requests":["local"]}"#);
    // Trigger ensure_repo to set up the local repo.
    let _ = sync::push(&env.cfg(), &env.storage);

    sync::force_pull(&env.cfg(), &env.storage).expect("force pull");
    assert_eq!(env.read_storage(), r#"{"requests":["remote"]}"#);
}

#[test]
fn push_already_in_sync_is_noop() {
    let env = TestEnv::new();
    env.write_storage(r#"{"requests":["v1"]}"#);

    sync::push(&env.cfg(), &env.storage).expect("push 1");

    // Push again without any changes — should succeed as no-op.
    let status = sync::push(&env.cfg(), &env.storage).expect("push 2");
    assert_eq!(status, SyncStatus::Ok);
}

