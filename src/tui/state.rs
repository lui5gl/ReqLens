use super::detail::format_request_detail;
use super::model::{DashboardStats, FilterTab, RequestDetail, RequestSummary, SortField};
use super::repo::{fetch_request_detail, fetch_requests, fetch_stats, open_readonly_conn};
use std::path::PathBuf;

pub struct TuiState {
    pub db_path: PathBuf,
    pub active_tab: FilterTab,
    pub sort_field: SortField,
    pub search_query: String,
    pub is_searching: bool,
    pub requests: Vec<RequestSummary>,
    pub selected_index: usize,
    pub selected_detail: Option<RequestDetail>,
    pub stats: DashboardStats,
    pub should_quit: bool,
    pub detail_scroll: u16,
    pub detail_notice: Option<String>,
    pub data_error: Option<String>,
}

impl TuiState {
    pub fn new(db_path: PathBuf) -> Self {
        let mut state = Self {
            db_path,
            active_tab: FilterTab::All,
            sort_field: SortField::Recent,
            search_query: String::new(),
            is_searching: false,
            requests: Vec::new(),
            selected_index: 0,
            selected_detail: None,
            stats: DashboardStats::default(),
            should_quit: false,
            detail_scroll: 0,
            detail_notice: None,
            data_error: None,
        };
        state.reload_data();
        state
    }

    pub fn reload_data(&mut self) {
        self.data_error = None;
        let Some(conn) = (match open_readonly_conn(&self.db_path) {
            Ok(connection) => connection,
            Err(error) => {
                self.data_error = Some(format!("No se pudo abrir SQLite: {error}"));
                return;
            }
        }) else {
            self.requests.clear();
            self.stats = DashboardStats::default();
            return;
        };

        self.stats = match fetch_stats(&conn) {
            Ok(stats) => stats,
            Err(error) => {
                self.data_error = Some(format!("No se pudieron leer las estadísticas: {error}"));
                return;
            }
        };

        self.requests = match fetch_requests(
            &conn,
            self.active_tab,
            self.sort_field,
            &self.search_query,
            100,
        ) {
            Ok(requests) => requests,
            Err(error) => {
                self.data_error = Some(format!("No se pudieron leer las solicitudes: {error}"));
                return;
            }
        };

        if self.requests.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.requests.len() {
            self.selected_index = self.requests.len() - 1;
        }
    }

    pub fn cycle_sort(&mut self) {
        self.sort_field = self.sort_field.next();
        self.selected_index = 0;
        self.reload_data();
    }

    pub fn add_search_char(&mut self, c: char) {
        self.search_query.push(c);
        self.selected_index = 0;
        self.reload_data();
    }

    pub fn pop_search_char(&mut self) {
        self.search_query.pop();
        self.selected_index = 0;
        self.reload_data();
    }

    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.is_searching = false;
        self.selected_index = 0;
        self.reload_data();
    }

    pub fn next_row(&mut self) {
        if !self.requests.is_empty() && self.selected_index + 1 < self.requests.len() {
            self.selected_index += 1;
        }
    }

    pub fn previous_row(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn set_tab(&mut self, tab: FilterTab) {
        self.active_tab = tab;
        self.selected_index = 0;
        self.reload_data();
    }

    pub fn toggle_detail(&mut self) {
        if self.selected_detail.is_some() {
            self.selected_detail = None;
            self.detail_scroll = 0;
            self.detail_notice = None;
            return;
        }

        let Some(summary) = self.requests.get(self.selected_index) else {
            return;
        };
        let conn = match open_readonly_conn(&self.db_path) {
            Ok(Some(connection)) => connection,
            Ok(None) => {
                self.detail_notice = Some("La base de datos todavía no existe".into());
                return;
            }
            Err(error) => {
                self.detail_notice = Some(format!("No se pudo abrir SQLite: {error}"));
                return;
            }
        };

        match fetch_request_detail(&conn, summary.id) {
            Ok(Some(detail)) => {
                self.selected_detail = Some(detail);
                self.detail_scroll = 0;
                self.detail_notice = None;
            }
            Ok(None) => {
                self.detail_notice = Some("La solicitud ya no está disponible".into());
            }
            Err(error) => {
                self.detail_notice = Some(format!("No se pudo leer el detalle: {error}"));
            }
        }
    }

    pub fn scroll_detail_up(&mut self) {
        if self.detail_scroll > 0 {
            self.detail_scroll -= 1;
        }
    }

    pub fn scroll_detail_down(&mut self) {
        self.detail_scroll = self
            .detail_scroll
            .saturating_add(1)
            .min(self.detail_max_scroll());
    }

    pub fn scroll_detail_page_up(&mut self, page_size: u16) {
        self.detail_scroll = self.detail_scroll.saturating_sub(page_size);
    }

    pub fn scroll_detail_page_down(&mut self, page_size: u16) {
        self.detail_scroll = self
            .detail_scroll
            .saturating_add(page_size)
            .min(self.detail_max_scroll());
    }

    pub fn detail_text(&self) -> Option<String> {
        self.selected_detail.as_ref().map(format_request_detail)
    }

    fn detail_max_scroll(&self) -> u16 {
        self.detail_text()
            .map(|text| text.lines().count().saturating_sub(1) as u16)
            .unwrap_or_default()
    }

    pub fn set_detail_notice(&mut self, notice: String) {
        self.detail_notice = Some(notice);
    }
}
