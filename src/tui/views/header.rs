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
            "ReqLens",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            match &config.source {
                TuiSource::Passive { .. } => "  PASSIVE HTTP OBSERVABILITY",
                TuiSource::Proxy { .. } => "  HTTP PROXY OBSERVABILITY",
            },
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  "),
        Span::styled(
            "LIVE",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let source_line = match &config.source {
        TuiSource::Passive {
            interface,
            server_ip,
            port,
        } => Line::from(vec![
            Span::styled("SOURCE ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{interface}"), Style::default().fg(Color::Yellow)),
            Span::styled("  ·  TARGET ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{}:{}",
                    server_ip.map_or_else(|| "any".to_string(), |ip| ip.to_string()),
                    port
                ),
                Style::default().fg(Color::White),
            ),
        ]),
        TuiSource::Proxy { listen, upstream } => Line::from(vec![
            Span::styled("LISTEN ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{listen}"), Style::default().fg(Color::Yellow)),
            Span::styled("  ->  UPSTREAM ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{upstream}"), Style::default().fg(Color::White)),
        ]),
    };

    let error_color = if state.stats.error_count > 0 {
        Color::Red
    } else {
        Color::Green
    };
    let stats_line = Line::from(vec![
        Span::styled("REQUESTS ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", state.stats.total_requests),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   ERRORS ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", state.stats.error_count),
            Style::default()
                .fg(error_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   AVG LATENCY ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format_latency(state.stats.avg_latency_ms),
            Style::default().fg(Color::Cyan),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" ReqLens ")
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(
        Paragraph::new(vec![title, source_line, stats_line]).block(block),
        area,
    );
}

fn format_latency(latency_ms: f64) -> String {
    if latency_ms == 0.0 {
        "—".to_string()
    } else {
        format!("{latency_ms:.1} ms")
    }
}
