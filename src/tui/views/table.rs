use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Row, Table};

use crate::tui::model::RequestSummary;
use crate::tui::state::TuiState;

pub fn render_table(frame: &mut Frame, area: Rect, state: &TuiState) {
    let header = Row::new(vec![
        "ID",
        "Hora UTC",
        "Método",
        "Status",
        "Latencia",
        "IP Cliente",
        "Path",
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(1);

    let visible_row_count = usize::from(area.height.saturating_sub(4)).max(1);
    let visible_start = state
        .selected_index
        .saturating_add(1)
        .saturating_sub(visible_row_count);
    let rows: Vec<Row> = state
        .requests
        .iter()
        .enumerate()
        .skip(visible_start)
        .take(visible_row_count)
        .map(|(idx, req)| {
            let is_selected = idx == state.selected_index;
            build_table_row(req, is_selected)
        })
        .collect();
    let visible_end = visible_start.saturating_add(rows.len());

    let widths = [
        Constraint::Length(8),
        Constraint::Length(14),
        Constraint::Length(9),
        Constraint::Length(8),
        Constraint::Length(11),
        Constraint::Length(17),
        Constraint::Min(20),
    ];

    let table = Table::new(rows, widths).header(header).block(
        Block::default().borders(Borders::ALL).title(format!(
            " Peticiones Capturadas ({}) | {}-{} ",
            state.requests.len(),
            visible_start.saturating_add(1),
            visible_end
        )),
    );

    frame.render_widget(table, area);
}

fn build_table_row(req: &RequestSummary, is_selected: bool) -> Row<'static> {
    let method_style = match req.method.as_str() {
        "GET" => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        "POST" => Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD),
        "PUT" | "PATCH" => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        "DELETE" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };

    let status_style = match req.resp_status {
        200..=299 => Style::default().fg(Color::Green),
        300..=399 => Style::default().fg(Color::Cyan),
        400..=499 => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        500..=599 => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::Gray),
    };

    let row_style = if is_selected {
        Style::default()
            .bg(Color::Rgb(40, 44, 52))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let time_str = req
        .timestamp
        .split('T')
        .nth(1)
        .unwrap_or(&req.timestamp)
        .to_string();

    Row::new(vec![
        Span::raw(format!("#{}", req.id)),
        Span::raw(time_str),
        Span::styled(req.method.clone(), method_style),
        Span::styled(format!("{}", req.resp_status), status_style),
        Span::raw(format!("{} ms", req.duration_ms)),
        Span::raw(req.client_ip.clone()),
        Span::raw(req.path.clone()),
    ])
    .style(row_style)
}
