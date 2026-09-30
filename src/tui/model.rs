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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodFilter {
    Any,
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl MethodFilter {
    pub const ALL: [MethodFilter; 6] = [
        MethodFilter::Any,
        MethodFilter::Get,
        MethodFilter::Post,
        MethodFilter::Put,
        MethodFilter::Patch,
        MethodFilter::Delete,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            MethodFilter::Any => "Cualquiera",
            MethodFilter::Get => "GET",
            MethodFilter::Post => "POST",
            MethodFilter::Put => "PUT",
            MethodFilter::Patch => "PATCH",
            MethodFilter::Delete => "DELETE",
        }
    }

    pub fn next(&self) -> Self {
        let index = Self::ALL.iter().position(|item| item == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()].clone()
    }
}

impl Default for MethodFilter {
    fn default() -> Self {
        Self::Any
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusFilter {
    Any,
    Errors,
    ClientErrors,
    ServerErrors,
}

impl StatusFilter {
    pub const ALL: [StatusFilter; 4] = [
        StatusFilter::Any,
        StatusFilter::Errors,
        StatusFilter::ClientErrors,
        StatusFilter::ServerErrors,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            StatusFilter::Any => "Cualquiera",
            StatusFilter::Errors => "Errores (>=400)",
            StatusFilter::ClientErrors => "4xx",
            StatusFilter::ServerErrors => "5xx",
        }
    }

    pub fn next(&self) -> Self {
        let index = Self::ALL.iter().position(|item| item == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()].clone()
    }
}

impl Default for StatusFilter {
    fn default() -> Self {
        Self::Any
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestFilters {
    pub method: MethodFilter,
    pub status: StatusFilter,
    pub path: String,
    pub min_duration_ms: Option<i64>,
}

impl Default for RequestFilters {
    fn default() -> Self {
        Self {
            method: MethodFilter::Any,
            status: StatusFilter::Any,
            path: String::new(),
            min_duration_ms: None,
        }
    }
}

impl RequestFilters {
    pub fn summary(&self) -> String {
        let mut values = Vec::new();
        if self.method != MethodFilter::Any {
            values.push(format!("Método: {}", self.method.label()));
        }
        if self.status != StatusFilter::Any {
            values.push(format!("Estado: {}", self.status.label()));
        }
        if !self.path.is_empty() {
            values.push(format!("Ruta: {}", self.path));
        }
        if let Some(duration) = self.min_duration_ms {
            values.push(format!("Desde: {duration} ms"));
        }
        if values.is_empty() {
            "sin filtros avanzados".to_string()
        } else {
            values.join("  ·  ")
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
