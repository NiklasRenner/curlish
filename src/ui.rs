use crate::app::{App, EditField, Mode};
use crate::model::{format_headers, UiArea};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

const STYLE_SELECTED: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
const STYLE_STATUS: Style = Style::new().fg(Color::Yellow);
const STYLE_RESPONSE: Style = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);
const STYLE_FOCUSED_BORDER: Style = Style::new().fg(Color::Cyan);
const KEY_HELP: &str = "WASD: areas  \u{2191}\u{2193}: navigate  E: edit  R: run  Ctrl+S: save  N: new  X: del  Q: quit";

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(frame.size());

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[0]);

    draw_request_list(frame, app, top_chunks[0]);
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
        Mode::HeaderEdit { index, editing_value } => {
            draw_header_edit(frame, app, *index, *editing_value);
        }
        Mode::BodyEditor { lines, cursor_row, cursor_col } => {
            draw_body_editor(frame, lines, *cursor_row, *cursor_col);
        }
        _ => {}
    }

    draw_status(frame, app);
}

fn area_block(title: &str, focused: bool) -> Block<'_> {
    let block = Block::default().title(title).borders(Borders::ALL);
    if focused {
        block.border_type(BorderType::Thick).border_style(STYLE_FOCUSED_BORDER)
    } else {
        block
    }
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
            ListItem::new(format!(" {}", req.name)).style(style)
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
        let body_display = if req.body.is_empty() { "(empty)" } else { &req.body };

        vec![
            field_line("Name", &req.name, current == EditField::Name),
            field_line("Method", &req.method.to_string(), current == EditField::Method),
            field_line("URL", &req.url, current == EditField::Url),
            field_line("Headers", &format_headers(&req.headers), current == EditField::Headers),
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
    let block = area_block("Response", focused);
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

fn draw_status(frame: &mut Frame<'_>, app: &App) {
    let area = frame.size();
    let bar = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };

    let text = match &app.mode {
        Mode::Normal => format!("[{}] | {KEY_HELP} | {}", app.focused_area.label(), app.status_line),
        Mode::Editing(field) => {
            let value = if app.input.is_empty() { "(type and Enter)" } else { &app.input };
            format!("EDIT {field:?}: {value}")
        }
        Mode::MethodPicker { filter, .. } => {
            let hint = if filter.is_empty() { "" } else { filter.as_str() };
            format!("METHOD | type to filter: {hint} | \u{2191}\u{2193}: select  Enter: confirm  Esc: cancel")
        }
        Mode::HeaderList { .. } => {
            String::from("HEADERS | \u{2191}\u{2193}: select  N: add  X: delete  E/Enter: edit  Esc: done")
        }
        Mode::HeaderEdit { editing_value, .. } => {
            let part = if *editing_value { "value" } else { "name" };
            format!("HEADER {part}: {} | Tab/Enter: next  Esc: cancel", app.input)
        }
        Mode::BodyEditor { .. } => {
            String::from("BODY | type freely  Esc/Ctrl+S: save & exit")
        }
    };

    frame.render_widget(
        Paragraph::new(text).style(STYLE_STATUS).wrap(Wrap { trim: true }),
        bar,
    );
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

fn draw_header_edit(frame: &mut Frame<'_>, app: &App, index: usize, editing_value: bool) {
    let area = centered_rect(60, 20, frame.size());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(format!("Edit Header #{}", index + 1))
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

    let lines = vec![
        Line::from(vec![
            Span::styled("Name:  ", name_style),
            Span::raw(display_name.to_string()),
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

