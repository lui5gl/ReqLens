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
        .constraints([Constraint::Length(45), Constraint::Min(20)])
        .split(area);

    let titles: Vec<Line> = FilterTab::ALL
        .iter()
        .map(|t| {
            let style = if *t == state.active_tab {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            Line::from(Span::styled(t.title(), style))
        })
        .collect();

    let selected_index = match state.active_tab {
        FilterTab::All => 0,
        FilterTab::Errors => 1,
        FilterTab::Slow => 2,
    };

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Vistas / Filtros [Tab] "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::UNDERLINED),
        )
        .select(selected_index);

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
            "Escribe para buscar... (Enter/Esc)".to_string()
        } else {
            "Presiona [/] para buscar".to_string()
        }
    } else {
        format!("\"{}\" (Esc para limpiar)", state.search_query)
    };

    let filter_info = Line::from(vec![
        Span::raw("🔍 Buscar: "),
        Span::styled(search_display, search_style),
        Span::raw(" | 🔄 Orden [s]: "),
        Span::styled(
            state.sort_field.label(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .title(" Búsqueda y Ordenamiento ");
    let search_widget = Paragraph::new(filter_info).block(search_block);
    frame.render_widget(search_widget, chunks[1]);
}
