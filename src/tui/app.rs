use base64::Engine;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};
use std::io::{self, Stdout};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use super::model::FilterTab;
use super::state::TuiState;
use super::views::render_ui;
use crate::error::{ReqLensError, Result};

const DEFAULT_TERMINAL_COLUMNS: u16 = 80;
const DEFAULT_TERMINAL_ROWS: u16 = 24;
const DETAIL_PAGE_SCROLL: u16 = 12;
const OSC52_PREFIX: &str = "\x1b]52;c;";
const OSC52_SUFFIX: &str = "\x07";
const INPUT_READER_ERROR: &str = "Failed to initialize input reader";
const INPUT_READER_HINT: &str = "ReqLens could not read keyboard input from this terminal. Run it from an interactive SSH/console TTY with TERM set (for example: export TERM=xterm), then retry.";

#[derive(Debug, Clone)]
pub enum TuiSource {
    Passive {
        interface: String,
        server_ip: Option<Ipv4Addr>,
        port: u16,
    },
    Proxy {
        listen: String,
        upstream: String,
    },
}

#[derive(Debug, Clone)]
pub struct TuiConfig {
    pub db_path: PathBuf,
    pub source: TuiSource,
}

pub fn run_tui_app(config: &TuiConfig) -> Result<()> {
    enable_raw_mode().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("ReqLens TUI requires an interactive terminal: {error}"),
        )
    })?;

    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(error.into());
    }

    let backend = CrosstermBackend::new(stdout);
    let terminal = create_terminal(backend);
    let mut terminal = match terminal {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            return Err(error);
        }
    };

    let mut state = TuiState::new(config.db_path.clone());
    let res = run_loop(&mut terminal, &mut state, config).map_err(describe_tui_input_error);

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    res
}

fn describe_tui_input_error(error: ReqLensError) -> ReqLensError {
    match error {
        ReqLensError::Io(error) if error.to_string().contains(INPUT_READER_ERROR) => {
            ReqLensError::Config(INPUT_READER_HINT.into())
        }
        error => error,
    }
}

fn create_terminal(
    backend: CrosstermBackend<Stdout>,
) -> Result<Terminal<CrosstermBackend<Stdout>>> {
    match crossterm::terminal::size() {
        Ok(_) => Ok(Terminal::new(backend)?),
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
            let fallback_area = Rect::new(0, 0, DEFAULT_TERMINAL_COLUMNS, DEFAULT_TERMINAL_ROWS);
            Ok(Terminal::with_options(
                backend,
                TerminalOptions {
                    viewport: Viewport::Fixed(fallback_area),
                },
            )?)
        }
        Err(error) => Err(error.into()),
    }
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    state: &mut TuiState,
    config: &TuiConfig,
) -> Result<()> {
    let tick_rate = Duration::from_millis(500);

    while !state.should_quit {
        terminal.draw(|frame| render_ui(frame, state, config))?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()?
                && key.kind != KeyEventKind::Release
            {
                handle_key_event(state, key);
            }
        } else if state.selected_detail.is_none() {
            state.reload_data();
        }
    }

    Ok(())
}

fn handle_key_event(state: &mut TuiState, key: KeyEvent) {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        state.should_quit = true;
        return;
    }

    let code = key.code;
    if state.selected_detail.is_some() {
        match code {
            KeyCode::Char('q') => state.should_quit = true,
            KeyCode::Esc | KeyCode::Enter => state.toggle_detail(),
            KeyCode::Up | KeyCode::Char('k') => state.scroll_detail_up(),
            KeyCode::Down | KeyCode::Char('j') => state.scroll_detail_down(),
            KeyCode::PageUp => state.scroll_detail_page_up(DETAIL_PAGE_SCROLL),
            KeyCode::PageDown => state.scroll_detail_page_down(DETAIL_PAGE_SCROLL),
            KeyCode::Char('c') => copy_detail_to_clipboard(state),
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
        KeyCode::Esc if !state.search_query.is_empty() => state.clear_search(),
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

fn copy_detail_to_clipboard(state: &mut TuiState) {
    let Some(detail_text) = state.detail_text() else {
        return;
    };
    let encoded_detail = base64::engine::general_purpose::STANDARD.encode(detail_text);

    match write_terminal_clipboard(&encoded_detail) {
        Ok(()) => state.set_detail_notice("Copia enviada al portapapeles del terminal".into()),
        Err(error) => state.set_detail_notice(format!("No se pudo copiar: {error}")),
    }
}

fn write_terminal_clipboard(encoded_detail: &str) -> io::Result<()> {
    use std::io::Write;

    let mut stdout = io::stdout();
    write!(stdout, "{OSC52_PREFIX}{encoded_detail}{OSC52_SUFFIX}")?;
    stdout.flush()
}
