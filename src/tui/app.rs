use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};
use std::time::Duration;

use super::model::FilterTab;
use super::state::TuiState;
use super::views::render_ui;
use crate::config::cli::AppConfig;
use crate::error::Result;

pub fn run_tui_app(config: &AppConfig) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = TuiState::new(config.db_path.clone());
    let res = run_loop(&mut terminal, &mut state, config);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    state: &mut TuiState,
    config: &AppConfig,
) -> Result<()> {
    let tick_rate = Duration::from_millis(500);

    while !state.should_quit {
        terminal.draw(|frame| render_ui(frame, state, config))?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                handle_key_event(state, key.code);
            }
        } else {
            state.reload_data();
        }
    }

    Ok(())
}

fn handle_key_event(state: &mut TuiState, code: KeyCode) {
    if state.selected_detail.is_some() {
        match code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => state.toggle_detail(),
            KeyCode::Up | KeyCode::Char('k') => state.scroll_detail_up(),
            KeyCode::Down | KeyCode::Char('j') => state.scroll_detail_down(),
            _ => {}
        }
        return;
    }

    if state.is_searching {
        match code {
            KeyCode::Esc => state.clear_search(),
            KeyCode::Enter => state.is_searching = false,
            KeyCode::Backspace => state.pop_search_char(),
            KeyCode::Char(c) => state.add_search_char(c),
            _ => {}
        }
        return;
    }

    match code {
        KeyCode::Char('q') => state.should_quit = true,
        KeyCode::Char('/') => state.is_searching = true,
        KeyCode::Char('s') | KeyCode::Char('o') => state.cycle_sort(),
        KeyCode::Esc => {
            if !state.search_query.is_empty() {
                state.clear_search();
            }
        }
        KeyCode::Down | KeyCode::Char('j') => state.next_row(),
        KeyCode::Up | KeyCode::Char('k') => state.previous_row(),
        KeyCode::Enter | KeyCode::Char(' ') => state.toggle_detail(),
        KeyCode::Char('1') => state.set_tab(FilterTab::All),
        KeyCode::Char('2') => state.set_tab(FilterTab::Errors),
        KeyCode::Char('3') => state.set_tab(FilterTab::Slow),
        KeyCode::Tab => {
            let next_tab = match state.active_tab {
                FilterTab::All => FilterTab::Errors,
                FilterTab::Errors => FilterTab::Slow,
                FilterTab::Slow => FilterTab::All,
            };
            state.set_tab(next_tab);
        }
        KeyCode::Char('r') => state.reload_data(),
        _ => {}
    }
}
