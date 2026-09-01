use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::tui::detail::format_request_detail;
use crate::tui::model::RequestDetail;

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
