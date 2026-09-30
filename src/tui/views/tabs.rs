use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};

use crate::tui::model::FilterTab;
use crate::tui::state::TuiState;

pub fn render_tabs(frame: &mut Frame, area: Rect, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(area);

    let titles = FilterTab::ALL.iter().map(|tab| {
        Line::from(Span::styled(
            tab.title(),
            Style::default().fg(if *tab == state.active_tab {
                Color::Yellow
            } else {
                Color::Gray
            }),
        ))
    });
    let selected_index = match state.active_tab {
        FilterTab::All => 0,
        FilterTab::Errors => 1,
        FilterTab::Slow => 2,
    };
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Filters "))
        .select(selected_index)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        );
    frame.render_widget(tabs, chunks[0]);

    let search_style = if state.is_searching {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if !state.search_query.is_empty() {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let search_display = if state.search_query.is_empty() {
        if state.is_searching {
            "escribe para buscar...".to_string()
        } else {
            "sin filtro".to_string()
        }
    } else {
        format!("\"{}\"", state.search_query)
    };

    let filter_info = Line::from(vec![
        Span::styled("Search ", Style::default().fg(Color::DarkGray)),
        Span::styled(search_display, search_style),
        Span::styled("  ·  Sort ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            state.sort_field.label(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
        Span::styled(state.filters.summary(), Style::default().fg(Color::Green)),
    ]);

    let lines = match state.data_error.as_deref() {
        Some(error) => vec![
            filter_info,
            Line::from(Span::styled(
                format!("Data error: {error}"),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )),
        ],
        None => vec![filter_info],
    };
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Query ")),
        chunks[1],
    );
}
