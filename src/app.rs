use crate::headers;
use crate::http;
use crate::model::{EnvVariable, Environment, HeaderEntry, HttpMethod, Request, RequestStore, ResponseSummary, UiArea};
use crate::storage;
use crate::sync::{self, SyncConfig, SyncStatus};
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
    /// Editing a single header's name or value inline (with autocomplete)
    HeaderEdit {
        index: usize,
        editing_value: bool, // false = name, true = value
        autocomplete_idx: Option<usize>, // currently highlighted suggestion
    },
    /// Multi-line body editor within the TUI
    BodyEditor {
        lines: Vec<String>,
        cursor_row: usize,
        cursor_col: usize,
    },
    /// Confirm delete: Up/Down to select, Enter to confirm
    ConfirmDelete { selected: usize },
    /// Confirm quit with unsaved changes
    ConfirmQuit { selected: usize },
    /// Environment editor: manage variables in the active environment
    EnvEditor { selected: usize },
    /// Editing a single env variable's key or value
    EnvVarEdit {
        index: usize,
        editing_value: bool,
    },
    /// Editing the environment name
    EnvNameEdit,
    /// Sync conflict: choose Keep local / Take remote / Cancel
    SyncConflict { selected: usize },
    /// Setup sync: enter repo URL
    SyncSetup,
    /// Sync error popup: show full error, dismiss with Esc/Enter
    SyncError { message: String },
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
    pub sync_config: Option<SyncConfig>,
    next_id: u64,
}

impl App {
    pub fn load() -> Result<Self> {
        let storage_path = storage::default_path();
        let sync_config = sync::load_config();

        // Try to pull latest before loading
        if let Some(ref cfg) = sync_config {
            let _ = sync::pull(cfg, &storage_path);
        }

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
            sync_config,
            next_id,
        })
    }

    // Key dispatch

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<AppAction> {
        match &self.mode.clone() {
            Mode::Normal => self.handle_normal(key),
            Mode::Editing(field) => self.handle_inline_edit(*field, key),
            Mode::MethodPicker { .. } => self.handle_method_picker(key),
            Mode::HeaderList { .. } => self.handle_header_list(key),
            Mode::HeaderEdit { .. } => self.handle_header_edit(key),
            Mode::BodyEditor { .. } => self.handle_body_editor(key),
            Mode::ConfirmDelete { .. } => self.handle_confirm_delete(key),
            Mode::ConfirmQuit { .. } => self.handle_confirm_quit(key),
            Mode::EnvEditor { .. } => self.handle_env_editor(key),
            Mode::EnvVarEdit { .. } => self.handle_env_var_edit(key),
            Mode::EnvNameEdit => self.handle_env_name_edit(key),
            Mode::SyncConflict { .. } => self.handle_sync_conflict(key),
            Mode::SyncSetup => self.handle_sync_setup(key),
            Mode::SyncError { .. } => self.handle_sync_error(key),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) -> Result<AppAction> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            self.save_store()?;
            return Ok(AppAction::Continue);
        }

        match key.code {
            KeyCode::Char('q') => {
                if storage::has_unsaved_changes(&self.storage_path, &self.store) {
                    self.mode = Mode::ConfirmQuit { selected: 1 };
                } else {
                    return Ok(AppAction::Quit);
                }
            }

            // WASD moves between areas
            KeyCode::Char('w') => self.focused_area = self.focused_area.up(),
            KeyCode::Char('a') => self.focused_area = self.focused_area.left(),
            KeyCode::Char('s') => self.focused_area = self.focused_area.down(),
            KeyCode::Char('d') => self.focused_area = self.focused_area.right(),

            // Up/Down navigates within the focused area
            KeyCode::Up => match self.focused_area {
                UiArea::Environment => self.cycle_environment(-1),
                UiArea::RequestList => self.move_selection(-1),
                UiArea::Details => self.move_field(-1),
                UiArea::Response => self.scroll_response(-1),
            },
            KeyCode::Down => match self.focused_area {
                UiArea::Environment => self.cycle_environment(1),
                UiArea::RequestList => self.move_selection(1),
                UiArea::Details => self.move_field(1),
                UiArea::Response => self.scroll_response(1),
            },

            // Actions
            KeyCode::Char('n') => {
                if self.focused_area == UiArea::Environment {
                    self.add_environment();
                } else {
                    self.add_request();
                }
            }
            KeyCode::Char('x') => {
                if self.focused_area == UiArea::Environment {
                    self.delete_environment();
                } else if !self.store.requests.is_empty() {
                    self.mode = Mode::ConfirmDelete { selected: 1 };
                }
            }
            KeyCode::Char('e') => {
                if self.focused_area == UiArea::Details {
                    self.start_edit();
                } else if self.focused_area == UiArea::Environment {
                    if self.store.active_environment.is_some() {
                        self.mode = Mode::EnvEditor { selected: 0 };
                    }
                }
            }
            KeyCode::Char('r') => self.execute_request()?,
            KeyCode::Char('S') => self.save_store()?,
            KeyCode::Char('c') => self.duplicate_request(),
            KeyCode::Char('g') => self.trigger_sync(),
            KeyCode::Char('G') => {
                self.input = self.sync_config.as_ref().map_or(String::new(), |c| c.repo_url.clone());
                self.mode = Mode::SyncSetup;
            }
            _ => {}
        }

        Ok(AppAction::Continue)
    }

    //  Inline text edit (Name, URL)

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

    // Method picker

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

    // Header list
    fn handle_header_list(&mut self, key: KeyEvent) -> Result<AppAction> {
        let header_count = self.current_request().map_or(0, |r| r.headers.len());
        let Mode::HeaderList { ref mut selected } = self.mode else {
            return Ok(AppAction::Continue);
        };

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
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
                        name: String::new(),
                        value: String::new(),
                    });
                    let idx = req.headers.len() - 1;
                    self.input = String::new();
                    self.mode = Mode::HeaderEdit { index: idx, editing_value: false, autocomplete_idx: None };
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
                    self.mode = Mode::HeaderEdit { index: sel, editing_value: false, autocomplete_idx: None };
                }
            }
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    // Header inline edit

    fn handle_header_edit(&mut self, key: KeyEvent) -> Result<AppAction> {
        let Mode::HeaderEdit { index, editing_value, autocomplete_idx } = self.mode else {
            return Ok(AppAction::Continue);
        };
        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::HeaderList { selected: index };
            }
            KeyCode::Enter => {
                if let Some(ai) = autocomplete_idx {
                    self.accept_header_suggestion(ai, index, editing_value);
                } else {
                    self.save_header_field(index, editing_value);
                }
            }
            KeyCode::Tab => {
                let suggestions = self.current_suggestions(editing_value, index);
                if let Some(ai) = autocomplete_idx {
                    self.accept_header_suggestion(ai, index, editing_value);
                } else if !suggestions.is_empty() {
                    self.mode = Mode::HeaderEdit { index, editing_value, autocomplete_idx: Some(0) };
                } else {
                    self.save_header_field(index, editing_value);
                }
            }
            KeyCode::Up => {
                let count = self.current_suggestions(editing_value, index).len();
                if count > 0 {
                    let new_idx = match autocomplete_idx {
                        Some(0) | None => count - 1,
                        Some(i) => i - 1,
                    };
                    self.mode = Mode::HeaderEdit { index, editing_value, autocomplete_idx: Some(new_idx) };
                }
            }
            KeyCode::Down => {
                let count = self.current_suggestions(editing_value, index).len();
                if count > 0 {
                    let new_idx = match autocomplete_idx {
                        None => 0,
                        Some(i) => (i + 1) % count,
                    };
                    self.mode = Mode::HeaderEdit { index, editing_value, autocomplete_idx: Some(new_idx) };
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.mode = Mode::HeaderEdit { index, editing_value, autocomplete_idx: None };
            }
            KeyCode::Char(ch) => {
                self.input.push(ch);
                self.mode = Mode::HeaderEdit { index, editing_value, autocomplete_idx: None };
            }
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    fn accept_header_suggestion(&mut self, suggestion_idx: usize, header_idx: usize, editing_value: bool) {
        let suggestions = self.current_suggestions(editing_value, header_idx);
        if let Some(s) = suggestions.get(suggestion_idx) {
            self.input = s.to_string();
        }
        self.mode = Mode::HeaderEdit { index: header_idx, editing_value, autocomplete_idx: None };
    }

    fn save_header_field(&mut self, index: usize, editing_value: bool) {
        let input_val = self.input.trim().to_string();
        if let Some(req) = self.current_request_mut() {
            if index < req.headers.len() {
                if !editing_value {
                    req.headers[index].name = input_val;
                    self.input = req.headers[index].value.clone();
                    self.mode = Mode::HeaderEdit { index, editing_value: true, autocomplete_idx: None };
                } else {
                    req.headers[index].value = input_val;
                    self.input.clear();
                    self.mode = Mode::HeaderList { selected: index };
                }
            }
        }
    }

    pub fn current_suggestions(&self, editing_value: bool, index: usize) -> Vec<&str> {
        if !editing_value {
            headers::filter_suggestions(headers::COMMON_HEADER_NAMES, &self.input)
        } else {
            let header_name = self
                .current_request()
                .and_then(|r| r.headers.get(index))
                .map(|h| h.name.as_str())
                .unwrap_or("");
            let values = headers::common_values_for(header_name);
            headers::filter_suggestions(values, &self.input)
        }
    }

    // Body editor

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

        // Ctrl+P to prettify JSON
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
            let raw = lines.join("\n");
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Ok(pretty) = serde_json::to_string_pretty(&parsed) {
                    *lines = pretty.lines().map(String::from).collect();
                    *cursor_row = 0;
                    *cursor_col = 0;
                }
            }
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

    // ── Confirm popups ─────────────────────────────────────────────

    fn handle_confirm_delete(&mut self, key: KeyEvent) -> Result<AppAction> {
        let Mode::ConfirmDelete { ref mut selected } = self.mode else {
            return Ok(AppAction::Continue);
        };
        match Self::handle_yes_no(key, selected) {
            Some(true) => {
                self.delete_request();
                self.mode = Mode::Normal;
            }
            Some(false) => {
                self.status_line = String::from("Delete cancelled");
                self.mode = Mode::Normal;
            }
            None => {}
        }
        Ok(AppAction::Continue)
    }

    fn handle_confirm_quit(&mut self, key: KeyEvent) -> Result<AppAction> {
        let Mode::ConfirmQuit { ref mut selected } = self.mode else {
            return Ok(AppAction::Continue);
        };
        match Self::handle_yes_no(key, selected) {
            Some(true) => return Ok(AppAction::Quit),
            Some(false) => self.mode = Mode::Normal,
            None => {}
        }
        Ok(AppAction::Continue)
    }

    /// Shared two-option popup logic. Returns Some(true) for confirm,
    /// Some(false) for cancel/esc, None if no decision yet.
    fn handle_yes_no(key: KeyEvent, selected: &mut usize) -> Option<bool> {
        match key.code {
            KeyCode::Up | KeyCode::Down => {
                *selected = 1 - *selected;
                None
            }
            KeyCode::Enter => Some(*selected == 0),
            KeyCode::Esc => Some(false),
            _ => None,
        }
    }

    // ── Environment editing

    fn handle_env_editor(&mut self, key: KeyEvent) -> Result<AppAction> {
        let active_idx = match self.store.active_environment {
            Some(i) => i,
            None => {
                self.mode = Mode::Normal;
                return Ok(AppAction::Continue);
            }
        };
        let var_count = self.store.environments.get(active_idx).map_or(0, |e| e.variables.len());
        let Mode::EnvEditor { ref mut selected } = self.mode else {
            return Ok(AppAction::Continue);
        };

        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Up => {
                if var_count > 0 {
                    *selected = if *selected == 0 { var_count - 1 } else { *selected - 1 };
                }
            }
            KeyCode::Down => {
                if var_count > 0 {
                    *selected = (*selected + 1) % var_count;
                }
            }
            KeyCode::Char('n') => {
                if let Some(env) = self.store.environments.get_mut(active_idx) {
                    env.variables.push(EnvVariable {
                        key: String::new(),
                        value: String::new(),
                    });
                    let idx = env.variables.len() - 1;
                    self.input = String::new();
                    self.mode = Mode::EnvVarEdit { index: idx, editing_value: false };
                }
            }
            KeyCode::Char('x') => {
                let sel = *selected;
                if let Some(env) = self.store.environments.get_mut(active_idx) {
                    if sel < env.variables.len() {
                        env.variables.remove(sel);
                    }
                }
                let new_count = self.store.environments.get(active_idx).map_or(0, |e| e.variables.len());
                if let Mode::EnvEditor { ref mut selected } = self.mode {
                    if *selected >= new_count && new_count > 0 {
                        *selected = new_count - 1;
                    }
                }
            }
            KeyCode::Char('e') | KeyCode::Enter => {
                let sel = *selected;
                if sel < var_count {
                    if let Some(env) = self.store.environments.get(active_idx) {
                        self.input = env.variables[sel].key.clone();
                    }
                    self.mode = Mode::EnvVarEdit { index: sel, editing_value: false };
                }
            }
            KeyCode::Char('r') => {
                // Rename environment
                if let Some(env) = self.store.environments.get(active_idx) {
                    self.input = env.name.clone();
                }
                self.mode = Mode::EnvNameEdit;
            }
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    fn handle_env_var_edit(&mut self, key: KeyEvent) -> Result<AppAction> {
        let Mode::EnvVarEdit { index, editing_value } = self.mode else {
            return Ok(AppAction::Continue);
        };
        let active_idx = self.store.active_environment.unwrap_or(0);

        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::EnvEditor { selected: index };
            }
            KeyCode::Tab | KeyCode::Enter => {
                let input_val = self.input.trim().to_string();
                if let Some(env) = self.store.environments.get_mut(active_idx) {
                    if index < env.variables.len() {
                        if !editing_value {
                            env.variables[index].key = input_val;
                            let next_val = env.variables[index].value.clone();
                            self.input = next_val;
                            self.mode = Mode::EnvVarEdit { index, editing_value: true };
                        } else {
                            env.variables[index].value = input_val;
                            self.input.clear();
                            self.mode = Mode::EnvEditor { selected: index };
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

    fn handle_env_name_edit(&mut self, key: KeyEvent) -> Result<AppAction> {
        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::EnvEditor { selected: 0 };
            }
            KeyCode::Enter => {
                let name = self.input.trim().to_string();
                if let Some(idx) = self.store.active_environment {
                    if let Some(env) = self.store.environments.get_mut(idx) {
                        env.name = name;
                    }
                }
                self.input.clear();
                self.mode = Mode::EnvEditor { selected: 0 };
            }
            KeyCode::Backspace => { self.input.pop(); }
            KeyCode::Char(ch) => self.input.push(ch),
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    fn add_environment(&mut self) {
        let name = format!("env-{}", self.store.environments.len() + 1);
        self.store.environments.push(Environment::new(&name));
        self.store.active_environment = Some(self.store.environments.len() - 1);
        self.status_line = format!("Environment \"{name}\" created");
    }

    fn delete_environment(&mut self) {
        if let Some(idx) = self.store.active_environment {
            if idx < self.store.environments.len() {
                self.store.environments.remove(idx);
                if self.store.environments.is_empty() {
                    self.store.active_environment = None;
                } else {
                    self.store.active_environment = Some(idx.min(self.store.environments.len() - 1));
                }
                self.status_line = String::from("Environment deleted");
            }
        }
    }

    // Navigation

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

    // Request CRUD

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

    fn duplicate_request(&mut self) {
        let Some(req) = self.current_request().cloned() else { return };
        let mut dup = req;
        dup.id = self.next_id;
        self.next_id += 1;
        dup.name = format!("{}-copy", dup.name);
        self.store.requests.insert(self.selected + 1, dup);
        self.selected += 1;
        self.status_line = "Request duplicated".into();
    }

    // Editing

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

    // HTTP execution

    fn execute_request(&mut self) -> Result<()> {
        let req = self.current_request().context("No request selected")?;
        if req.url.trim().is_empty() {
            self.status_line = "URL is empty".into();
            return Ok(());
        }

        let env_vars = self.active_env_vars();
        match http::execute_request(req, &env_vars) {
            Ok(resp) => {
                self.status_line = "Request completed".into();
                self.response = Some(resp);
            }
            Err(err) => self.status_line = format!("Request failed: {err}"),
        }
        Ok(())
    }

    // Persistence

    fn save_store(&mut self) -> Result<()> {
        storage::save(&self.storage_path, &self.store)?;

        // Auto-push after save if sync is configured
        if let Some(ref cfg) = self.sync_config {
            match sync::push(cfg, &self.storage_path) {
                Ok(SyncStatus::Ok) => {
                    self.status_line = format!("Saved & synced to {}", self.storage_path.display());
                }
                Ok(SyncStatus::Conflict) => {
                    self.status_line = "Saved locally, sync conflict detected".into();
                    self.mode = Mode::SyncConflict { selected: 2 }; // default to Cancel
                }
                Ok(SyncStatus::Disabled) => {
                    self.show_sync_error("Sync unavailable (no git repo initialized)");
                }
                Err(e) => {
                    self.show_sync_error(&format!("Push failed: {e}"));
                }
            }
        } else {
            self.status_line = format!("Saved to {}", self.storage_path.display());
        }
        Ok(())
    }

    // -- Sync --

    fn trigger_sync(&mut self) {
        let Some(ref cfg) = self.sync_config.clone() else {
            self.status_line = "No sync configured (Shift+G to set up)".into();
            return;
        };

        self.status_line = "Syncing...".into();

        // First save current state
        if let Err(e) = storage::save(&self.storage_path, &self.store) {
            self.show_sync_error(&format!("Save failed: {e}"));
            return;
        }

        // Push
        match sync::push(&cfg, &self.storage_path) {
            Ok(SyncStatus::Ok) => {
                self.status_line = "Synced successfully".into();
            }
            Ok(SyncStatus::Conflict) => {
                self.status_line = "Sync conflict".into();
                self.mode = Mode::SyncConflict { selected: 2 };
            }
            Ok(SyncStatus::Disabled) => {
                self.show_sync_error("Sync unavailable (no git repo initialized)");
            }
            Err(e) => {
                self.show_sync_error(&format!("Sync failed: {e}"));
            }
        }
    }

    fn handle_sync_conflict(&mut self, key: KeyEvent) -> Result<AppAction> {
        let Mode::SyncConflict { ref mut selected } = self.mode else {
            return Ok(AppAction::Continue);
        };
        match key.code {
            KeyCode::Up => *selected = if *selected == 0 { 2 } else { *selected - 1 },
            KeyCode::Down => *selected = (*selected + 1) % 3,
            KeyCode::Enter => {
                let choice = *selected;
                self.mode = Mode::Normal;
                match choice {
                    0 => {
                        // Keep local — force push
                        if let Some(ref cfg) = self.sync_config {
                            match sync::force_push(cfg, &self.storage_path) {
                                Ok(()) => self.status_line = "Forced push — local version kept".into(),
                                Err(e) => self.show_sync_error(&format!("Force push failed: {e}")),
                            }
                        }
                    }
                    1 => {
                        // Take remote — force pull and reload
                        if let Some(ref cfg) = self.sync_config {
                            match sync::force_pull(cfg, &self.storage_path) {
                                Ok(()) => {
                                    match storage::load_or_default(&self.storage_path) {
                                        Ok(store) => {
                                            self.store = store;
                                            self.selected = 0;
                                            self.next_id = self.store.requests.iter().map(|r| r.id).max().unwrap_or(0) + 1;
                                            self.status_line = "Loaded remote version".into();
                                        }
                                        Err(e) => self.show_sync_error(&format!("Failed to reload: {e}")),
                                    }
                                }
                                Err(e) => self.show_sync_error(&format!("Force pull failed: {e}")),
                            }
                        }
                    }
                    _ => {
                        self.status_line = "Sync conflict unresolved".into();
                    }
                }
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status_line = "Sync conflict unresolved".into();
            }
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    fn handle_sync_setup(&mut self, key: KeyEvent) -> Result<AppAction> {
        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let url = self.input.trim().to_string();
                self.input.clear();
                self.mode = Mode::Normal;

                if url.is_empty() {
                    // Disable sync
                    self.sync_config = None;
                    let _ = std::fs::remove_file(sync::config_path());
                    self.status_line = "Sync disabled".into();
                } else {
                    let cfg = SyncConfig {
                        repo_url: url,
                        branch: String::from("main"),
                    };
                    match sync::save_config(&cfg) {
                        Ok(()) => {
                            if !sync::is_git_available() {
                                self.show_sync_error("git is not installed");
                            } else if let Err(e) = sync::init(&cfg, &self.storage_path) {
                                self.show_sync_error(&format!("Init failed: {e}"));
                            } else {
                                self.status_line = "Sync configured".into();
                            }
                            self.sync_config = Some(cfg);
                        }
                        Err(e) => self.show_sync_error(&format!("Failed to save config: {e}")),
                    }
                }
            }
            KeyCode::Backspace => { self.input.pop(); }
            KeyCode::Char(ch) => self.input.push(ch),
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    fn handle_sync_error(&mut self, key: KeyEvent) -> Result<AppAction> {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(AppAction::Continue)
    }

    fn show_sync_error(&mut self, message: &str) {
        self.status_line = "Sync error".into();
        self.mode = Mode::SyncError { message: message.to_string() };
    }

    // ── Accessors ─────────────────────────────────────────────────

    pub fn current_request(&self) -> Option<&Request> {
        self.store.requests.get(self.selected)
    }

    pub fn current_request_mut(&mut self) -> Option<&mut Request> {
        self.store.requests.get_mut(self.selected)
    }

    pub fn active_env_vars(&self) -> Vec<EnvVariable> {
        self.store
            .active_environment
            .and_then(|i| self.store.environments.get(i))
            .map(|e| e.variables.clone())
            .unwrap_or_default()
    }

    pub fn active_env_name(&self) -> &str {
        self.store
            .active_environment
            .and_then(|i| self.store.environments.get(i))
            .map(|e| e.name.as_str())
            .unwrap_or("(none)")
    }

    fn cycle_environment(&mut self, delta: isize) {
        let len = self.store.environments.len();
        if len == 0 {
            return;
        }
        let current = self.store.active_environment.unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(len as isize) as usize;
        self.store.active_environment = Some(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtered_methods_empty_filter_returns_all() {
        let result = App::filtered_methods("");
        assert_eq!(result.len(), HttpMethod::ALL.len());
    }

    #[test]
    fn filtered_methods_narrows_by_substring() {
        let result = App::filtered_methods("P");
        assert!(result.contains(&HttpMethod::Post));
        assert!(result.contains(&HttpMethod::Put));
        assert!(result.contains(&HttpMethod::Patch));
        assert!(!result.contains(&HttpMethod::Get));
    }

    #[test]
    fn filtered_methods_case_insensitive() {
        let result = App::filtered_methods("get");
        assert_eq!(result, vec![HttpMethod::Get]);
    }

    #[test]
    fn filtered_methods_no_match() {
        let result = App::filtered_methods("ZZZZZ");
        assert!(result.is_empty());
    }

    #[test]
    fn current_field_maps_indices() {
        let app = App {
            store: RequestStore::default(),
            selected: 0,
            field_index: 0,
            mode: Mode::Normal,
            input: String::new(),
            status_line: String::new(),
            response: None,
            storage_path: PathBuf::from("test"),
            focused_area: UiArea::RequestList,
            response_scroll: 0,
            sync_config: None,
            next_id: 1,
        };

        let fields = [
            EditField::Name,
            EditField::Method,
            EditField::Url,
            EditField::Headers,
            EditField::Body,
        ];
        for (i, expected) in fields.iter().enumerate() {
            let a = App {
                field_index: i,
                store: app.store.clone(),
                selected: app.selected,
                mode: Mode::Normal,
                input: String::new(),
                status_line: String::new(),
                response: None,
                storage_path: PathBuf::from("test"),
                focused_area: UiArea::RequestList,
                response_scroll: 0,
                sync_config: None,
                next_id: 1,
            };
            assert_eq!(a.current_field(), *expected);
        }
    }

    #[test]
    fn current_field_out_of_bounds_returns_body() {
        let app = App {
            store: RequestStore::default(),
            selected: 0,
            field_index: 99,
            mode: Mode::Normal,
            input: String::new(),
            status_line: String::new(),
            response: None,
            storage_path: PathBuf::from("test"),
            focused_area: UiArea::RequestList,
            response_scroll: 0,
            sync_config: None,
            next_id: 1,
        };
        assert_eq!(app.current_field(), EditField::Body);
    }
}
