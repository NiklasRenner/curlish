use crate::http;
use crate::model::{HeaderEntry, HttpMethod, Request, RequestStore, ResponseSummary, UiArea};
use crate::storage;
use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

const FIELD_COUNT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Name,
    Method,
    Url,
    Headers,
    Body,
}

#[derive(Debug, Clone)]
pub enum Mode {
    Normal,
    /// Simple inline text edit (Name, URL)
    Editing(EditField),
    /// Method picker: Up/Down to select, type to filter, Enter to confirm
    MethodPicker {
        filter: String,
        selected: usize,
    },
    /// Header list browser: Up/Down to select, n/x/e to add/delete/edit, Esc to leave
    HeaderList {
        selected: usize,
    },
    /// Editing a single header's name or value inline
    HeaderEdit {
        index: usize,
        editing_value: bool, // false = name, true = value
    },
    /// Multi-line body editor within the TUI
    BodyEditor {
        lines: Vec<String>,
        cursor_row: usize,
        cursor_col: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    Continue,
    Quit,
}

pub struct App {
    pub store: RequestStore,
    pub selected: usize,
    pub field_index: usize,
    pub mode: Mode,
    pub input: String,
    pub status_line: String,
    pub response: Option<ResponseSummary>,
    pub storage_path: PathBuf,
    pub focused_area: UiArea,
    pub response_scroll: u16,
    next_id: u64,
}

impl App {
    pub fn load() -> Result<Self> {
        let storage_path = storage::default_path();
        let store = storage::load_or_default(&storage_path)?;
        let next_id = store.requests.iter().map(|r| r.id).max().unwrap_or(0) + 1;

        Ok(Self {
            store,
            selected: 0,
            field_index: 0,
            mode: Mode::Normal,
            input: String::new(),
            status_line: String::new(),
            response: None,
            storage_path,
            focused_area: UiArea::RequestList,
            response_scroll: 0,
            next_id,
        })
    }

    // ── Key dispatch ──────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<AppAction> {
        match &self.mode.clone() {
            Mode::Normal => self.handle_normal(key),
            Mode::Editing(field) => self.handle_inline_edit(*field, key),
            Mode::MethodPicker { .. } => self.handle_method_picker(key),
            Mode::HeaderList { .. } => self.handle_header_list(key),
            Mode::HeaderEdit { .. } => self.handle_header_edit(key),
            Mode::BodyEditor { .. } => self.handle_body_editor(key),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) -> Result<AppAction> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            self.save_store()?;
            return Ok(AppAction::Continue);
        }

        match key.code {
            KeyCode::Char('q') => return Ok(AppAction::Quit),

            // WASD moves between areas
            KeyCode::Char('w') => self.focused_area = self.focused_area.up(),
            KeyCode::Char('a') => self.focused_area = self.focused_area.left(),
            KeyCode::Char('s') => self.focused_area = self.focused_area.down(),
            KeyCode::Char('d') => self.focused_area = self.focused_area.right(),

            // Up/Down navigates within the focused area
            KeyCode::Up => match self.focused_area {
                UiArea::RequestList => self.move_selection(-1),
                UiArea::Details => self.move_field(-1),
                UiArea::Response => self.scroll_response(-1),
            },
            KeyCode::Down => match self.focused_area {
                UiArea::RequestList => self.move_selection(1),
                UiArea::Details => self.move_field(1),
                UiArea::Response => self.scroll_response(1),
            },

            // Actions
            KeyCode::Char('n') => self.add_request(),
            KeyCode::Char('x') => self.delete_request(),
            KeyCode::Char('e') => {
                if self.focused_area == UiArea::Details {
                    self.start_edit();
                }
            }
            KeyCode::Char('r') => self.execute_request()?,
            KeyCode::Char('S') => self.save_store()?,
            _ => {}
        }

        Ok(AppAction::Continue)
    }

    // ── Inline text edit (Name, URL) ──────────────────────────────

    fn handle_inline_edit(&mut self, field: EditField, key: KeyEvent) -> Result<AppAction> {
        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                self.apply_inline_input(field)?;
                self.input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => { self.input.pop(); }
            KeyCode::Char(ch) => self.input.push(ch),
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    // ── Method picker ─────────────────────────────────────────────

    fn handle_method_picker(&mut self, key: KeyEvent) -> Result<AppAction> {
        let Mode::MethodPicker { ref mut filter, ref mut selected } = self.mode else {
            return Ok(AppAction::Continue);
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                let filtered = Self::filtered_methods(filter);
                if let Some(&method) = filtered.get(*selected) {
                    if let Some(req) = self.store.requests.get_mut(self.selected) {
                        req.method = method;
                    }
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Up => {
                let count = Self::filtered_methods(filter).len();
                if count > 0 {
                    *selected = if *selected == 0 { count - 1 } else { *selected - 1 };
                }
            }
            KeyCode::Down => {
                let count = Self::filtered_methods(filter).len();
                if count > 0 {
                    *selected = (*selected + 1) % count;
                }
            }
            KeyCode::Backspace => {
                filter.pop();
                *selected = 0;
            }
            KeyCode::Char(ch) => {
                filter.push(ch);
                *selected = 0;
            }
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    pub fn filtered_methods(filter: &str) -> Vec<HttpMethod> {
        let f = filter.to_ascii_uppercase();
        HttpMethod::ALL
            .iter()
            .copied()
            .filter(|m| f.is_empty() || m.as_str().contains(&f))
            .collect()
    }

    // ── Header list ───────────────────────────────────────────────

    fn handle_header_list(&mut self, key: KeyEvent) -> Result<AppAction> {
        let header_count = self.current_request().map_or(0, |r| r.headers.len());
        let Mode::HeaderList { ref mut selected } = self.mode else {
            return Ok(AppAction::Continue);
        };

        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Up => {
                if header_count > 0 {
                    *selected = if *selected == 0 { header_count - 1 } else { *selected - 1 };
                }
            }
            KeyCode::Down => {
                if header_count > 0 {
                    *selected = (*selected + 1) % header_count;
                }
            }
            KeyCode::Char('n') => {
                if let Some(req) = self.current_request_mut() {
                    req.headers.push(HeaderEntry {
                        name: String::from("Header-Name"),
                        value: String::from("value"),
                    });
                    let idx = req.headers.len() - 1;
                    self.input = req.headers[idx].name.clone();
                    self.mode = Mode::HeaderEdit { index: idx, editing_value: false };
                }
            }
            KeyCode::Char('x') => {
                let sel = *selected;
                if let Some(req) = self.current_request_mut() {
                    if sel < req.headers.len() {
                        req.headers.remove(sel);
                    }
                }
                let new_count = self.current_request().map_or(0, |r| r.headers.len());
                if let Mode::HeaderList { ref mut selected } = self.mode {
                    if *selected >= new_count && new_count > 0 {
                        *selected = new_count - 1;
                    }
                }
            }
            KeyCode::Char('e') | KeyCode::Enter => {
                let sel = *selected;
                if sel < header_count {
                    if let Some(req) = self.current_request() {
                        self.input = req.headers[sel].name.clone();
                    }
                    self.mode = Mode::HeaderEdit { index: sel, editing_value: false };
                }
            }
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    // ── Header inline edit ────────────────────────────────────────

    fn handle_header_edit(&mut self, key: KeyEvent) -> Result<AppAction> {
        let Mode::HeaderEdit { index, editing_value } = self.mode else {
            return Ok(AppAction::Continue);
        };
        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::HeaderList { selected: index };
            }
            KeyCode::Tab | KeyCode::Enter => {
                // Save current part and move to next or finish
                let input_val = self.input.trim().to_string();
                if let Some(req) = self.current_request_mut() {
                    if index < req.headers.len() {
                        if !editing_value {
                            req.headers[index].name = input_val;
                            let next_val = req.headers[index].value.clone();
                            self.input = next_val;
                            self.mode = Mode::HeaderEdit { index, editing_value: true };
                        } else {
                            req.headers[index].value = input_val;
                            self.input.clear();
                            self.mode = Mode::HeaderList { selected: index };
                        }
                    }
                }
            }
            KeyCode::Backspace => { self.input.pop(); }
            KeyCode::Char(ch) => self.input.push(ch),
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    // ── Body editor ───────────────────────────────────────────────

    fn handle_body_editor(&mut self, key: KeyEvent) -> Result<AppAction> {
        let Mode::BodyEditor { ref mut lines, ref mut cursor_row, ref mut cursor_col } = self.mode else {
            return Ok(AppAction::Continue);
        };

        // Ctrl+S in body editor saves body and exits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            let body = lines.join("\n");
            if let Some(req) = self.store.requests.get_mut(self.selected) {
                req.body = body;
            }
            self.mode = Mode::Normal;
            self.status_line = String::from("Body saved");
            return Ok(AppAction::Continue);
        }

        match key.code {
            KeyCode::Esc => {
                let body = lines.join("\n");
                if let Some(req) = self.store.requests.get_mut(self.selected) {
                    req.body = body;
                }
                self.mode = Mode::Normal;
                self.status_line = String::from("Body saved");
            }
            KeyCode::Enter => {
                // Split current line at cursor
                let tail = lines[*cursor_row].split_off(*cursor_col);
                lines.insert(*cursor_row + 1, tail);
                *cursor_row += 1;
                *cursor_col = 0;
            }
            KeyCode::Backspace => {
                if *cursor_col > 0 {
                    lines[*cursor_row].remove(*cursor_col - 1);
                    *cursor_col -= 1;
                } else if *cursor_row > 0 {
                    let removed = lines.remove(*cursor_row);
                    *cursor_row -= 1;
                    *cursor_col = lines[*cursor_row].len();
                    lines[*cursor_row].push_str(&removed);
                }
            }
            KeyCode::Left => {
                if *cursor_col > 0 {
                    *cursor_col -= 1;
                }
            }
            KeyCode::Right => {
                if *cursor_col < lines[*cursor_row].len() {
                    *cursor_col += 1;
                }
            }
            KeyCode::Up => {
                if *cursor_row > 0 {
                    *cursor_row -= 1;
                    *cursor_col = (*cursor_col).min(lines[*cursor_row].len());
                }
            }
            KeyCode::Down => {
                if *cursor_row + 1 < lines.len() {
                    *cursor_row += 1;
                    *cursor_col = (*cursor_col).min(lines[*cursor_row].len());
                }
            }
            KeyCode::Char(ch) => {
                lines[*cursor_row].insert(*cursor_col, ch);
                *cursor_col += 1;
            }
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    // ── Navigation ────────────────────────────────────────────────

    fn move_selection(&mut self, delta: isize) {
        let len = self.store.requests.len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
    }

    fn move_field(&mut self, delta: isize) {
        self.field_index =
            (self.field_index as isize + delta).rem_euclid(FIELD_COUNT as isize) as usize;
    }

    fn scroll_response(&mut self, delta: isize) {
        let new = self.response_scroll as isize + delta;
        self.response_scroll = new.max(0) as u16;
    }

    pub fn current_field(&self) -> EditField {
        match self.field_index {
            0 => EditField::Name,
            1 => EditField::Method,
            2 => EditField::Url,
            3 => EditField::Headers,
            _ => EditField::Body,
        }
    }

    // ── Request CRUD ──────────────────────────────────────────────

    fn add_request(&mut self) {
        self.store.requests.push(Request::new(self.next_id));
        self.next_id += 1;
        self.selected = self.store.requests.len() - 1;
        self.status_line = "New request added".into();
    }

    fn delete_request(&mut self) {
        if self.store.requests.is_empty() {
            return;
        }
        self.store.requests.remove(self.selected);
        if self.selected >= self.store.requests.len() && !self.store.requests.is_empty() {
            self.selected = self.store.requests.len() - 1;
        }
        self.status_line = "Request deleted".into();
    }

    // ── Editing ───────────────────────────────────────────────────

    fn start_edit(&mut self) {
        let Some(req) = self.current_request() else { return };
        let field = self.current_field();
        match field {
            EditField::Name | EditField::Url => {
                self.input = match field {
                    EditField::Name => req.name.clone(),
                    EditField::Url => req.url.clone(),
                    _ => unreachable!(),
                };
                self.mode = Mode::Editing(field);
            }
            EditField::Method => {
                self.mode = Mode::MethodPicker {
                    filter: String::new(),
                    selected: req.method.index(),
                };
            }
            EditField::Headers => {
                self.mode = Mode::HeaderList { selected: 0 };
            }
            EditField::Body => {
                let body = req.body.clone();
                let lines: Vec<String> = if body.is_empty() {
                    vec![String::new()]
                } else {
                    body.lines().map(String::from).collect()
                };
                self.mode = Mode::BodyEditor {
                    lines,
                    cursor_row: 0,
                    cursor_col: 0,
                };
            }
        }
    }

    fn apply_inline_input(&mut self, field: EditField) -> Result<()> {
        let input = self.input.clone();
        let req = self.current_request_mut().context("No request selected")?;
        match field {
            EditField::Name => req.name = input.trim().to_string(),
            EditField::Url => req.url = input.trim().to_string(),
            _ => {}
        }
        Ok(())
    }

    // ── HTTP execution ────────────────────────────────────────────

    fn execute_request(&mut self) -> Result<()> {
        let req = self.current_request().context("No request selected")?;
        if req.url.trim().is_empty() {
            self.status_line = "URL is empty".into();
            return Ok(());
        }

        match http::execute_request(req) {
            Ok(resp) => {
                self.status_line = "Request completed".into();
                self.response = Some(resp);
            }
            Err(err) => self.status_line = format!("Request failed: {err}"),
        }
        Ok(())
    }

    // ── Persistence ───────────────────────────────────────────────

    fn save_store(&mut self) -> Result<()> {
        storage::save(&self.storage_path, &self.store)?;
        self.status_line = format!("Saved to {}", self.storage_path.display());
        Ok(())
    }

    // ── Accessors ─────────────────────────────────────────────────

    pub fn current_request(&self) -> Option<&Request> {
        self.store.requests.get(self.selected)
    }

    pub fn current_request_mut(&mut self) -> Option<&mut Request> {
        self.store.requests.get_mut(self.selected)
    }
}

