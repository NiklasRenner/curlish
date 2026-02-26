# curlish

A lightweight TUI for saving and running HTTP requests, sitting between curl and Postman.

## Features (MVP)
- List, edit, save, and execute HTTP requests
- JSON storage in `.curlish.json`
- Keyboard-first navigation (WASD)

## Keys
- `W/A/S/D`: navigate between areas (Requests, Details, Response)
- `↑/↓`: navigate within the focused area
- `E`: edit field (in Details)
- `R`: run request
- `Ctrl+S`: save requests
- `N`: new request
- `X`: delete request
- `Q`: quit

## Storage format
Requests are saved to `.curlish.json` in the project root.

## Run
```powershell
cargo run
```

## Test
```powershell
cargo test
```

