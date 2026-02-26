use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

mod app;
mod headers;
mod http;
mod model;
mod storage;
mod sync;
mod ui;

fn main() -> Result<()> {
    setup_git_ssh_env();

    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal);
    restore_terminal(&mut terminal)?;

    result
}

/// Set `HOME` and `GIT_SSH_COMMAND` so child git/ssh processes spawned by
/// git-sync-rs can locate SSH keys on Windows.
fn setup_git_ssh_env() {
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
fn find_ssh_key(dir: &std::path::Path) -> Option<PathBuf> {
    for name in ["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"] {
        let key = dir.join(name);
        if key.is_file() {
            return Some(key);
        }
    }
    // Fallback: any `id_*` file that isn't a .pub
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        let n = p.file_name()?.to_string_lossy().to_string();
        if n.starts_with("id_") && !n.ends_with(".pub") && p.is_file() {
            return Some(p);
        }
    }
    None
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), terminal::LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let mut app = app::App::load()?;

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if app.handle_key(key)? == app::AppAction::Quit {
                    break;
                }
            }
        }
    }

    Ok(())
}
