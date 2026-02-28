use crate::app::{App, EditField, Mode};
use crate::model::{format_headers, format_query_params, UiArea};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Padding, Paragraph, Wrap};
use ratatui::Frame;

const STYLE_SELECTED: Style = Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD);
const STYLE_TITLE: Style = Style::new().fg(Color::White);
const STYLE_RESPONSE: Style = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);
const STYLE_BORDER: Style = Style::new().fg(Color::Green);
const STYLE_FOCUSED_BORDER: Style = Style::new().fg(Color::Magenta);

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(frame.size());

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[0]);

    // Split the left column: environment selector (small) + request list
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(top_chunks[0]);

    draw_environment(frame, app, left_chunks[0]);
    draw_request_list(frame, app, left_chunks[1]);
    draw_details(frame, app, top_chunks[1]);
    draw_response(frame, app, chunks[1]);

    // Draw overlays for special edit modes
    match &app.mode {
        Mode::MethodPicker { filter, selected } => {
            draw_method_picker(frame, filter, *selected);
        }
        Mode::HeaderList { selected } => {
            draw_header_list(frame, app, *selected);
        }
        Mode::HeaderEdit { index, editing_value, autocomplete_idx } => {
            draw_header_edit(frame, app, *index, *editing_value, *autocomplete_idx);
        }
        Mode::QueryParamList { selected } => {
            draw_query_param_list(frame, app, *selected);
        }
        Mode::QueryParamEdit { index, editing_value } => {
            draw_query_param_edit(frame, app, *index, *editing_value);
        }
        Mode::BodyEditor { lines, cursor_row, cursor_col } => {
            draw_body_editor(frame, lines, *cursor_row, *cursor_col);
        }
        Mode::ConfirmDelete { selected } => {
            draw_confirm_delete(frame, app, *selected);
        }
        Mode::ConfirmQuit { selected } => {
            draw_confirm_quit(frame, *selected);
        }
        Mode::EnvEditor { selected } => {
            draw_env_editor(frame, app, *selected);
        }
        Mode::EnvVarEdit { index, editing_value } => {
            draw_env_var_edit(frame, app, *index, *editing_value);
        }
        Mode::EnvNameEdit => {
            draw_env_name_edit(frame, app);
        }
        Mode::SyncConflict { selected } => {
            draw_sync_conflict(frame, *selected);
        }
        Mode::SyncSetup => {
            draw_sync_setup(frame, app);
        }
        Mode::SyncError { message } => {
            draw_sync_error(frame, message);
        }
        Mode::Keymap => {
            draw_keymap(frame);
        }
        _ => {}
    }
}

fn area_block(title: &str, focused: bool) -> Block<'_> {
    let block = Block::default()
        .title(title)
        .title_style(STYLE_TITLE)
        .borders(Borders::ALL)
        .padding(Padding::new(1, 1, 0, 0));
    if focused {
        block.border_type(BorderType::Thick).border_style(STYLE_FOCUSED_BORDER)
    } else {
        block.border_style(STYLE_BORDER)
    }
}

fn draw_environment(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.focused_area == UiArea::Environment;
    let env_name = app.active_env_name();
    let block = area_block("Env", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let style = if focused { STYLE_SELECTED } else { Style::default() };
    let text = Line::from(Span::styled(format!("{env_name}"), style));
    frame.render_widget(Paragraph::new(text), inner);
}

fn draw_request_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.focused_area == UiArea::RequestList;
    let items: Vec<ListItem> = app
        .store
        .requests
        .iter()
        .enumerate()
        .map(|(i, req)| {
            let style = if i == app.selected { STYLE_SELECTED } else { Style::default() };
            ListItem::new(format!("{}", req.name)).style(style)
        })
        .collect();

    let list = List::new(items).block(area_block("Requests", focused));
    frame.render_widget(list, area);
}

fn draw_details(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.focused_area == UiArea::Details;
    let block = area_block("Details", focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = if let Some(req) = app.current_request() {
        let current = app.current_field();
        let name_display = if req.name.is_empty() { "(unnamed)" } else { &req.name };
        let url_display = if req.url.is_empty() { "(no url)" } else { &req.url };
        let query_display = format_query_params(&req.query_params);
        let headers_display = format_headers(&req.headers);
        let body_display = if req.body.is_empty() { "(empty)" } else { &req.body };

        vec![
            field_line("Name", name_display, current == EditField::Name),
            field_line("Method", &req.method.to_string(), current == EditField::Method),
            field_line("URL", url_display, current == EditField::Url),
            field_line("Params", &query_display, current == EditField::QueryParams),
            field_line("Headers", &headers_display, current == EditField::Headers),
            field_line("Body", body_display, current == EditField::Body),
        ]
    } else {
        vec![Line::from("No requests")]
    };

    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: true }),
        inner,
    );
}

fn draw_response(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.focused_area == UiArea::Response;
    let block = area_block("Response", focused)
        .title_bottom(Line::from(format!(" {} ", app.status_line)).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = if let Some(resp) = &app.response {
        let mut out = vec![Line::from(Span::styled(&resp.status, STYLE_RESPONSE))];
        if !resp.headers.is_empty() {
            let summary = resp
                .headers
                .iter()
                .take(5)
                .map(|h| format!("{}: {}", h.name, h.value))
                .collect::<Vec<_>>()
                .join(" | ");
            out.push(Line::from(summary));
        }
        out.push(Line::from(""));
        out.extend(resp.body.lines().map(|l| Line::from(l.to_string())));
        out
    } else {
        vec![Line::from("Run a request to see the response.")]
    };

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((app.response_scroll, 0))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn draw_keymap(frame: &mut Frame<'_>) {
    let area = centered_rect(50, 60, frame.size());
    frame.render_widget(Clear, area);

    let bindings = [
        ("W / A / S / D", "Navigate areas"),
        ("↑ / ↓",         "Navigate items"),
        ("E / Enter",     "Edit selected field"),
        ("R",             "Run request"),
        ("Ctrl+S",        "Save to disk"),
        ("N",             "New request"),
        ("C",             "Copy request"),
        ("X",             "Delete request"),
        ("G",             "Sync"),
        ("Shift+G",       "Sync setup"),
        ("K",             "Show keybinds"),
        ("Q / Esc",       "Quit"),
    ];

    let items: Vec<ListItem> = bindings
        .iter()
        .map(|(key, desc)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {key:<16}"), STYLE_SELECTED),
                Span::raw(desc.to_string()),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title("Keybinds | Esc: close")
            .title_style(STYLE_TITLE)
            .borders(Borders::ALL)
            .border_style(STYLE_FOCUSED_BORDER),
    );
    frame.render_widget(list, area);
}

fn field_line(label: &str, value: &str, selected: bool) -> Line<'static> {
    let style = if selected { STYLE_SELECTED } else { Style::default() };

    Line::from(vec![
        Span::styled(format!("{label}: "), style),
        Span::raw(value.to_string()),
    ])
}

// ── Overlay helpers ───────────────────────────────────────────

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vert[1])[1]
}

fn draw_method_picker(frame: &mut Frame<'_>, filter: &str, selected: usize) {
    let area = centered_rect(30, 40, frame.size());
    frame.render_widget(Clear, area);

    let filtered = App::filtered_methods(filter);
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let style = if i == selected { STYLE_SELECTED } else { Style::default() };
            ListItem::new(format!("  {}", m.as_str())).style(style)
        })
        .collect();

    let title = if filter.is_empty() {
        String::from("Method")
    } else {
        format!("Method [{}]", filter)
    };

    let list = List::new(items).block(
        Block::default()
            .title(title)
            .padding(Padding::new(1, 1, 0, 0))
            .borders(Borders::ALL)
            .border_style(STYLE_FOCUSED_BORDER),
    );
    frame.render_widget(list, area);
}

fn draw_header_list(frame: &mut Frame<'_>, app: &App, selected: usize) {
    let area = centered_rect(60, 50, frame.size());
    frame.render_widget(Clear, area);

    let items: Vec<ListItem> = app
        .current_request()
        .map(|req| {
            req.headers
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let style = if i == selected { STYLE_SELECTED } else { Style::default() };
                    ListItem::new(format!("  {}: {}", h.name, h.value)).style(style)
                })
                .collect()
        })
        .unwrap_or_default();

    let count = items.len();
    let list = List::new(items).block(
        Block::default()
            .title(format!("Headers ({count}) | N:add X:del E:edit"))
            .borders(Borders::ALL)
            .border_style(STYLE_FOCUSED_BORDER),
    );
    frame.render_widget(list, area);
}

fn draw_header_edit(frame: &mut Frame<'_>, app: &App, index: usize, editing_value: bool, autocomplete_idx: Option<usize>) {
    let suggestions = app.current_suggestions(editing_value, index);
    let suggestion_count = suggestions.len();
    // Size the popup taller if there are suggestions to show
    let height_pct = if suggestion_count > 0 { 50 } else { 20 };

    let area = centered_rect(60, height_pct, frame.size());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(format!("Edit Header #{} | Tab/\u{2191}\u{2193}: autocomplete  Enter: confirm", index + 1))
        .borders(Borders::ALL)
        .border_style(STYLE_FOCUSED_BORDER);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (name, value) = app
        .current_request()
        .and_then(|r| r.headers.get(index))
        .map(|h| (h.name.as_str(), h.value.as_str()))
        .unwrap_or(("", ""));

    let name_style = if !editing_value { STYLE_SELECTED } else { Style::default() };
    let val_style = if editing_value { STYLE_SELECTED } else { Style::default() };

    let display_name = if !editing_value { &app.input } else { name };
    let display_value = if editing_value { &app.input } else { value };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Name:  ", name_style),
            Span::raw(display_name.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Value: ", val_style),
            Span::raw(display_value.to_string()),
        ]),
    ];

    if suggestion_count > 0 {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "\u{2500}\u{2500} suggestions \u{2500}\u{2500}",
            Style::default().fg(Color::DarkGray),
        )));
        for (i, s) in suggestions.iter().enumerate() {
            let style = if autocomplete_idx == Some(i) {
                STYLE_SELECTED
            } else {
                Style::default().fg(Color::DarkGray)
            };
            lines.push(Line::from(Span::styled(format!("  {s}"), style)));
        }
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_query_param_list(frame: &mut Frame<'_>, app: &App, selected: usize) {
    let area = centered_rect(60, 50, frame.size());
    frame.render_widget(Clear, area);

    let items: Vec<ListItem> = app
        .current_request()
        .map(|req| {
            req.query_params
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let style = if i == selected { STYLE_SELECTED } else { Style::default() };
                    let key = if p.key.is_empty() { "(key)" } else { &p.key };
                    let val = if p.value.is_empty() { "(value)" } else { &p.value };
                    ListItem::new(format!("  {key} = {val}")).style(style)
                })
                .collect()
        })
        .unwrap_or_default();

    let count = items.len();
    let list = List::new(items).block(
        Block::default()
            .title(format!("Query Params ({count}) | N:add X:del E:edit"))
            .borders(Borders::ALL)
            .border_style(STYLE_FOCUSED_BORDER),
    );
    frame.render_widget(list, area);
}

fn draw_query_param_edit(frame: &mut Frame<'_>, app: &App, index: usize, editing_value: bool) {
    let area = centered_rect(60, 20, frame.size());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(format!("Edit Param #{}", index + 1))
        .borders(Borders::ALL)
        .border_style(STYLE_FOCUSED_BORDER);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (key, value) = app
        .current_request()
        .and_then(|r| r.query_params.get(index))
        .map(|p| (p.key.as_str(), p.value.as_str()))
        .unwrap_or(("", ""));

    let key_style = if !editing_value { STYLE_SELECTED } else { Style::default() };
    let val_style = if editing_value { STYLE_SELECTED } else { Style::default() };

    let display_key = if !editing_value { &app.input } else { key };
    let display_value = if editing_value { &app.input } else { value };

    let lines = vec![
        Line::from(vec![
            Span::styled("Key:   ", key_style),
            Span::raw(display_key.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Value: ", val_style),
            Span::raw(display_value.to_string()),
        ]),
    ];

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_body_editor(frame: &mut Frame<'_>, lines: &[String], cursor_row: usize, cursor_col: usize) {
    let area = centered_rect(80, 70, frame.size());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title("Body Editor | Esc/Ctrl+S: save & exit")
        .padding(Padding::new(1, 1, 0, 0))
        .borders(Borders::ALL)
        .border_style(STYLE_FOCUSED_BORDER);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let display_lines: Vec<Line> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if i == cursor_row {
                // Show cursor position with a highlighted character
                let before = &line[..cursor_col.min(line.len())];
                let cursor_char = line.get(cursor_col..cursor_col + 1).unwrap_or(" ");
                let after = if cursor_col < line.len() { &line[cursor_col + 1..] } else { "" };
                Line::from(vec![
                    Span::raw(before.to_string()),
                    Span::styled(
                        cursor_char.to_string(),
                        Style::default().bg(Color::White).fg(Color::Black),
                    ),
                    Span::raw(after.to_string()),
                ])
            } else {
                Line::from(line.as_str().to_string())
            }
        })
        .collect();

    let scroll = if cursor_row >= inner.height as usize {
        (cursor_row - inner.height as usize + 1) as u16
    } else {
        0
    };

    frame.render_widget(
        Paragraph::new(Text::from(display_lines)).scroll((scroll, 0)),
        inner,
    );
}

fn draw_confirm_delete(frame: &mut Frame<'_>, app: &App, selected: usize) {
    let name = app.current_request().map_or("?", |r| r.name.as_str());
    draw_confirm_popup(frame, &format!("Delete \"{name}\"?"), &["Yes", "No"], selected);
}

fn draw_confirm_quit(frame: &mut Frame<'_>, selected: usize) {
    draw_confirm_popup(frame, "Unsaved changes!", &["Quit", "Cancel"], selected);
}

fn draw_confirm_popup(frame: &mut Frame<'_>, title: &str, options: &[&str], selected: usize) {
    let area = centered_rect(30, 20, frame.size());
    frame.render_widget(Clear, area);

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let style = if i == selected { STYLE_SELECTED } else { Style::default() };
            ListItem::new(format!("  {label}")).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(title.to_string())
            .borders(Borders::ALL)
            .border_style(STYLE_FOCUSED_BORDER),
    );
    frame.render_widget(list, area);
}

fn draw_env_editor(frame: &mut Frame<'_>, app: &App, selected: usize) {
    let area = centered_rect(60, 50, frame.size());
    frame.render_widget(Clear, area);

    let env_name = app.active_env_name();
    let items: Vec<ListItem> = app
        .store
        .active_environment
        .and_then(|i| app.store.environments.get(i))
        .map(|env| {
            env.variables
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let style = if i == selected { STYLE_SELECTED } else { Style::default() };
                    let key_display = if v.key.is_empty() { "(key)" } else { &v.key };
                    let val_display = if v.value.is_empty() { "(value)" } else { &v.value };
                    ListItem::new(format!("  {key_display} = {val_display}")).style(style)
                })
                .collect()
        })
        .unwrap_or_default();

    let count = items.len();
    let list = List::new(items).block(
        Block::default()
            .title(format!("{env_name} ({count} vars) | N:add X:del E:edit R:rename"))
            .borders(Borders::ALL)
            .border_style(STYLE_FOCUSED_BORDER),
    );
    frame.render_widget(list, area);
}

fn draw_env_var_edit(frame: &mut Frame<'_>, app: &App, index: usize, editing_value: bool) {
    let area = centered_rect(60, 20, frame.size());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(format!("Edit Variable #{}", index + 1))
        .borders(Borders::ALL)
        .border_style(STYLE_FOCUSED_BORDER);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (key, value) = app
        .store
        .active_environment
        .and_then(|i| app.store.environments.get(i))
        .and_then(|e| e.variables.get(index))
        .map(|v| (v.key.as_str(), v.value.as_str()))
        .unwrap_or(("", ""));

    let key_style = if !editing_value { STYLE_SELECTED } else { Style::default() };
    let val_style = if editing_value { STYLE_SELECTED } else { Style::default() };

    let display_key = if !editing_value { &app.input } else { key };
    let display_value = if editing_value { &app.input } else { value };

    let lines = vec![
        Line::from(vec![
            Span::styled("Key:   ", key_style),
            Span::raw(display_key.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Value: ", val_style),
            Span::raw(display_value.to_string()),
        ]),
    ];

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_env_name_edit(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(40, 15, frame.size());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title("Rename Environment")
        .borders(Borders::ALL)
        .border_style(STYLE_FOCUSED_BORDER);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![Line::from(vec![
        Span::styled("Name: ", STYLE_SELECTED),
        Span::raw(app.input.to_string()),
    ])];

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_sync_conflict(frame: &mut Frame<'_>, selected: usize) {
    let area = centered_rect(35, 25, frame.size());
    frame.render_widget(Clear, area);

    let options = ["Keep local", "Take remote", "Cancel"];
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let style = if i == selected { STYLE_SELECTED } else { Style::default() };
            ListItem::new(format!("  {label}")).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title("Sync Conflict")
            .borders(Borders::ALL)
            .border_style(STYLE_FOCUSED_BORDER),
    );
    frame.render_widget(list, area);
}

fn draw_sync_setup(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(60, 15, frame.size());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title("Sync Setup (empty to disable)")
        .borders(Borders::ALL)
        .border_style(STYLE_FOCUSED_BORDER);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![Line::from(vec![
        Span::styled("Repo URL: ", STYLE_SELECTED),
        Span::raw(app.input.to_string()),
    ])];

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_sync_error(frame: &mut Frame<'_>, message: &str) {
    let area = centered_rect(60, 30, frame.size());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title("Sync Error")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![];
    for line in message.lines() {
        lines.push(Line::from(Span::raw(line.to_string())));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press Esc or Enter to dismiss",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}
