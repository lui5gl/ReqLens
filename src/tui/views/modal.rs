use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::tui::detail::format_request_detail;
use crate::tui::model::RequestDetail;
use crate::tui::state::TuiState;

pub fn render_detail_modal(
    frame: &mut Frame,
    area: Rect,
    detail: &RequestDetail,
    scroll: u16,
    notice: Option<&str>,
) {
    let popup_area = centered_rect(85, 80, area);
    frame.render_widget(Clear, popup_area);

    let title = match notice {
        Some(message) => format!(" {message} "),
        None => " Detalle: [c] copiar, [PgUp/PgDn] pagina, [Esc/Enter] cerrar ".into(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(format_request_detail(detail))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, popup_area);
}

pub fn render_filter_modal(frame: &mut Frame, area: Rect, state: &TuiState) {
    let popup_area = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup_area);
    let Some(filters) = state.draft_filters.as_ref() else {
        return;
    };

    let fields = [
        ("Método", filters.method.label().to_string()),
        ("Estado", filters.status.label().to_string()),
        ("Ruta contiene", display_value(&filters.path, "cualquiera")),
        (
            "Duración mínima",
            filters
                .min_duration_ms
                .map(|value| format!("{value} ms"))
                .unwrap_or_else(|| "cualquiera".to_string()),
        ),
    ];
    let mut lines = vec![Line::from(Span::styled(
        "Tab/↑↓ mover   Espacio cambiar   Supr limpiar campo",
        Style::default().fg(Color::DarkGray),
    ))];
    for (index, (label, value)) in fields.iter().enumerate() {
        let style = if index == state.filter_field {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{label:<18}"), style),
            Span::styled(value.clone(), style),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter aplicar   Esc cancelar",
        Style::default().fg(Color::Cyan),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Filtros avanzados ")
        .border_style(Style::default().fg(Color::Yellow));
    frame.render_widget(Paragraph::new(lines).block(block), popup_area);
}

fn display_value(value: &str, empty_label: &str) -> String {
    if value.is_empty() {
        empty_label.to_string()
    } else {
        value.to_string()
    }
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
