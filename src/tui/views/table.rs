use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};

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

    let visible_row_count = usize::from(area.height.saturating_sub(2)).max(1);
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

    let title = if state.requests.is_empty() {
        "REQUESTS  0".to_string()
    } else {
        format!(
            "REQUESTS  {}  ·  showing {}-{}",
            state.requests.len(),
            visible_start.saturating_add(1),
            visible_end
        )
    };
    let table = Table::new(rows, widths).header(header).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(table, area);
    if state.requests.is_empty() {
        let table_body = Block::default().borders(Borders::ALL).inner(area);
        let body_area = Rect {
            y: table_body.y.saturating_add(2),
            height: table_body.height.saturating_sub(2),
            ..table_body
        };
        let empty_area = centered_message_area(body_area);
        let message = if state.search_query.is_empty() {
            "Esperando peticiones..."
        } else {
            "Ninguna petición coincide con este filtro"
        };
        frame.render_widget(
            Paragraph::new(message)
                .style(Style::default().fg(Color::DarkGray))
                .centered(),
            empty_area,
        );
    }
}

fn centered_message_area(area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(vertical[1])[1]
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

    let selection_marker = if is_selected { "▸" } else { " " };
    Row::new(vec![
        Span::raw(format!("{selection_marker}{}", req.id)),
        Span::raw(time_str),
        Span::styled(req.method.clone(), method_style),
        Span::styled(format!("{}", req.resp_status), status_style),
        Span::raw(format!("{} ms", req.duration_ms)),
        Span::raw(req.client_ip.clone()),
        Span::raw(req.path.clone()),
    ])
    .style(row_style)
}
