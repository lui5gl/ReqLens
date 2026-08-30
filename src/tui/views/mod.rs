pub mod footer;
pub mod header;
pub mod modal;
pub mod table;
pub mod tabs;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};

use crate::config::cli::AppConfig;
use crate::tui::state::TuiState;

pub fn render_ui(frame: &mut Frame, state: &TuiState, config: &AppConfig) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(frame.area());

    header::render_header(frame, chunks[0], state, config);
    tabs::render_tabs(frame, chunks[1], state);
    table::render_table(frame, chunks[2], state);
    footer::render_footer(frame, chunks[3]);

    if let Some(detail) = &state.selected_detail {
        modal::render_detail_modal(frame, frame.area(), detail, state.detail_scroll);
    }
}
