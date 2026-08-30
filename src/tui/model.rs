#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterTab {
    All,
    Errors,
    Slow,
}

impl FilterTab {
    pub const ALL: [FilterTab; 3] = [FilterTab::All, FilterTab::Errors, FilterTab::Slow];

    pub fn title(&self) -> &'static str {
        match self {
            FilterTab::All => "1: Todos",
            FilterTab::Errors => "2: Errores (≥400)",
            FilterTab::Slow => "3: Lentos (≥500ms)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortField {
    #[default]
    Recent,
    Slowest,
    StatusDesc,
    Oldest,
}

impl SortField {
    pub fn next(&self) -> Self {
        match self {
            SortField::Recent => SortField::Slowest,
            SortField::Slowest => SortField::StatusDesc,
            SortField::StatusDesc => SortField::Oldest,
            SortField::Oldest => SortField::Recent,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SortField::Recent => "Recientes",
            SortField::Slowest => "Más Lentos",
            SortField::StatusDesc => "Mayor Status",
            SortField::Oldest => "Más Antiguos",
        }
    }

    pub fn sql_order_by(&self) -> &'static str {
        match self {
            SortField::Recent => "ORDER BY id DESC",
            SortField::Slowest => "ORDER BY duration_ms DESC, id DESC",
            SortField::StatusDesc => "ORDER BY resp_status DESC, id DESC",
            SortField::Oldest => "ORDER BY id ASC",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RequestSummary {
    pub id: i64,
    pub timestamp: String,
    pub duration_ms: i64,
    pub client_ip: String,
    pub method: String,
    pub path: String,
    pub resp_status: u16,
}

#[derive(Debug, Clone)]
pub struct RequestDetail {
    pub id: i64,
    pub timestamp: String,
    pub duration_ms: i64,
    pub client_ip: String,
    pub client_ua: Option<String>,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub req_headers: String,
    pub req_body: Option<String>,
    pub resp_status: u16,
    pub resp_headers: String,
    pub resp_body: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DashboardStats {
    pub total_requests: i64,
    pub error_count: i64,
    pub avg_latency_ms: f64,
}
