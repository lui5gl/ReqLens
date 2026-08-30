use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::tui::model::RequestDetail;

pub fn render_detail_modal(frame: &mut Frame, area: Rect, detail: &RequestDetail, scroll: u16) {
    let popup_area = centered_rect(85, 80, area);
    frame.render_widget(Clear, popup_area);

    let status_color = if detail.resp_status >= 400 {
        Color::Red
    } else {
        Color::Green
    };
    let content = vec![
        Line::from(vec![
            Span::styled(
                format!("Petición #{} ", detail.id),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("[{} {}] ", detail.method, detail.path),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("Status: {} ", detail.resp_status),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("Latencia: {} ms", detail.duration_ms),
                Style::default().fg(Color::Magenta),
            ),
        ]),
        Line::from(vec![Span::raw(format!(
            "Timestamp: {} | IP Cliente: {} | UA: {}",
            detail.timestamp,
            detail.client_ip,
            detail.client_ua.as_deref().unwrap_or("-")
        ))]),
        Line::from(Span::raw("─".repeat(80))),
        Line::from(Span::styled(
            "--- Request Headers (Permitidos) ---",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(detail.req_headers.clone()),
        Line::from(""),
        Line::from(Span::styled(
            "--- Request Body (Con Redacción Fail-Safe) ---",
            Style::default().fg(Color::Green),
        )),
        Line::from(detail.req_body.as_deref().unwrap_or("(vacío)")),
        Line::from(""),
        Line::from(Span::styled(
            "--- Response Headers ---",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(detail.resp_headers.clone()),
        Line::from(""),
        Line::from(Span::styled(
            "--- Response Body ---",
            Style::default().fg(Color::Green),
        )),
        Line::from(detail.resp_body.as_deref().unwrap_or("(vacío)")),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Detalle de Petición HTTP (Presiona [Esc]/[Enter] para cerrar, [↑/↓] scroll) ")
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
