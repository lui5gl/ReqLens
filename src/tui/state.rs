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
        };
        state.reload_data();
        state
    }

    pub fn reload_data(&mut self) {
        if let Ok(Some(conn)) = open_readonly_conn(&self.db_path) {
            if let Ok(stats) = fetch_stats(&conn) {
                self.stats = stats;
            }
            if let Ok(reqs) = fetch_requests(
                &conn,
                self.active_tab,
                self.sort_field,
                &self.search_query,
                100,
            ) {
                self.requests = reqs;
                if self.requests.is_empty() {
                    self.selected_index = 0;
                } else if self.selected_index >= self.requests.len() {
                    self.selected_index = self.requests.len() - 1;
                }
            }
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

        if let Some(summary) = self.requests.get(self.selected_index)
            && let Ok(Some(conn)) = open_readonly_conn(&self.db_path)
            && let Ok(Some(detail)) = fetch_request_detail(&conn, summary.id)
        {
            self.selected_detail = Some(detail);
            self.detail_scroll = 0;
            self.detail_notice = None;
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
