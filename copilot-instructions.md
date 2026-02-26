# Copilot Guidelines (curlish)

## Scope
- Apply to all new code and edits in this repo.

## Project Context
- Language: Rust (edition 2024)
- A TUI tool between curl and Postman for customizing, saving and executing HTTP requests.
- Key crates: `ratatui`, `crossterm`, `reqwest` (blocking), `serde`/`serde_json`, `anyhow`.
- Entry point: `src/main.rs`; modules: `app`, `ui`, `model`, `http`, `storage`.
- Requests are persisted as JSON in `.curlish.json`.

## Code Style
- Prefer `.into()` over `.to_string()` for simple conversions.
- Keep functions small and focused; group related methods under section comments.
- Use idiomatic Rust error handling (`Result`, `?`, `anyhow::Context`).
- Avoid `unwrap`/`expect` in non-test code.

## Architecture
- Add new modules under `src/` and register via `mod` in `main.rs`.
- Avoid global state; pass dependencies explicitly.
- Navigation is keyboard-first using WASD (not vim-style hjkl).
- Only process `KeyEventKind::Press` to avoid doubled input on Windows.

## Testing
- Add unit tests in the same module using `#[cfg(test)]`.
- Keep tests deterministic and fast.

## Security
- Avoid unsafe code unless explicitly required and documented.
- Validate external input before use.


