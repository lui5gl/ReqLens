use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::app::{TuiConfig, TuiSource};
use crate::tui::state::TuiState;

pub fn render_header(frame: &mut Frame, area: Rect, state: &TuiState, config: &TuiConfig) {
    let title = Line::from(vec![
        Span::styled(
            " ReqLens ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(match &config.source {
            TuiSource::Passive { .. } => " | Passive HTTP Observability",
            TuiSource::Proxy { .. } => " | HTTP Proxy Observability",
        }),
    ]);
    let source_line = match &config.source {
        TuiSource::Passive {
            interface,
            server_ip,
            port,
        } => Line::from(vec![
            Span::styled(
                format!("Interface: {interface} | "),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!(
                    "Server: {} | Port: {port}",
                    server_ip.map_or_else(|| "any".to_string(), |ip| ip.to_string())
                ),
                Style::default().fg(Color::Green),
            ),
        ]),
        TuiSource::Proxy { listen, upstream } => Line::from(vec![
            Span::styled(
                format!("Listen: {listen} "),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("-> "),
            Span::styled(
                format!("Upstream: {upstream} "),
                Style::default().fg(Color::Green),
            ),
        ]),
    };

    let error_color = if state.stats.error_count > 0 {
        Color::Red
    } else {
        Color::Green
    };
    let stats_line = Line::from(vec![
        Span::raw("Total: "),
        Span::styled(
            format!("{} ", state.stats.total_requests),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("| Errores: "),
        Span::styled(
            format!("{} ", state.stats.error_count),
            Style::default()
                .fg(error_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("| Latencia Promedio: "),
        Span::styled(
            format!("{:.1} ms ", state.stats.avg_latency_ms),
            Style::default().fg(Color::Magenta),
        ),
        Span::raw("| WAL: "),
        Span::styled("Activo", Style::default().fg(Color::Green)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let header_widget = Paragraph::new(vec![title, source_line, stats_line]).block(block);
    frame.render_widget(header_widget, area);
}
