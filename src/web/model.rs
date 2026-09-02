use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestFilter {
    All,
    Errors,
    Slow,
}

impl RequestFilter {
    pub fn from_query(value: Option<&str>) -> Self {
        match value {
            Some("errors") => Self::Errors,
            Some("slow") => Self::Slow,
            _ => Self::All,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSortField {
    Id,
    Timestamp,
    Method,
    Status,
    Duration,
    ClientIp,
    Path,
}

impl RequestSortField {
    pub fn from_query(value: Option<&str>) -> Self {
        match value {
            Some("timestamp") => Self::Timestamp,
            Some("method") => Self::Method,
            Some("status") => Self::Status,
            Some("duration") => Self::Duration,
            Some("client_ip") => Self::ClientIp,
            Some("path") => Self::Path,
            _ => Self::Id,
        }
    }

    fn sql_column(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Timestamp => "timestamp",
            Self::Method => "method",
            Self::Status => "resp_status",
            Self::Duration => "duration_ms",
            Self::ClientIp => "client_ip",
            Self::Path => "path",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSortDirection {
    Ascending,
    Descending,
}

impl RequestSortDirection {
    pub fn from_query(value: Option<&str>) -> Self {
        match value {
            Some("asc") => Self::Ascending,
            _ => Self::Descending,
        }
    }

    fn sql_keyword(self) -> &'static str {
        match self {
            Self::Ascending => "ASC",
            Self::Descending => "DESC",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestSort {
    field: RequestSortField,
    direction: RequestSortDirection,
}

impl RequestSort {
    pub fn from_query(field: Option<&str>, direction: Option<&str>) -> Self {
        Self {
            field: RequestSortField::from_query(field),
            direction: RequestSortDirection::from_query(direction),
        }
    }

    pub fn sql_order_by(self) -> String {
        format!(
            "ORDER BY {} {}, id DESC",
            self.field.sql_column(),
            self.direction.sql_keyword()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestRuleField {
    Id,
    Method,
    Status,
    Duration,
    ClientIp,
    Path,
    Query,
    Timestamp,
}

impl RequestRuleField {
    pub fn from_query(value: &str) -> Option<Self> {
        match value {
            "id" => Some(Self::Id),
            "method" => Some(Self::Method),
            "status" => Some(Self::Status),
            "duration" => Some(Self::Duration),
            "client_ip" => Some(Self::ClientIp),
            "path" => Some(Self::Path),
            "query" => Some(Self::Query),
            "timestamp" => Some(Self::Timestamp),
            _ => None,
        }
    }

    pub fn sql_column(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Method => "method",
            Self::Status => "resp_status",
            Self::Duration => "duration_ms",
            Self::ClientIp => "client_ip",
            Self::Path => "path",
            Self::Query => "COALESCE(query, '')",
            Self::Timestamp => "timestamp",
        }
    }

    pub(crate) fn is_numeric(self) -> bool {
        matches!(self, Self::Id | Self::Status | Self::Duration)
    }

    fn is_timestamp(self) -> bool {
        matches!(self, Self::Timestamp)
    }

    fn supports(self, operator: RequestRuleOperator) -> bool {
        if self.is_numeric() || self.is_timestamp() {
            return !matches!(
                operator,
                RequestRuleOperator::Contains | RequestRuleOperator::NotContains | RequestRuleOperator::StartsWith
            );
        }
        matches!(
            operator,
            RequestRuleOperator::Is | RequestRuleOperator::Contains | RequestRuleOperator::StartsWith
                | RequestRuleOperator::NotEqual | RequestRuleOperator::NotContains
        )
    }

    fn value_is_valid(self, value: &str) -> bool {
        if self.is_numeric() {
            return value.parse::<i64>().is_ok();
        }
        if self.is_timestamp() {
            return value.len() >= 16 && value.as_bytes().get(10) == Some(&b'T');
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestRuleOperator {
    Is,
    NotEqual,
    Contains,
    NotContains,
    StartsWith,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
}

impl RequestRuleOperator {
    pub fn from_query(value: &str) -> Option<Self> {
        match value {
            "is" => Some(Self::Is),
            "not_equal" => Some(Self::NotEqual),
            "contains" => Some(Self::Contains),
            "not_contains" => Some(Self::NotContains),
            "starts_with" => Some(Self::StartsWith),
            "gt" => Some(Self::GreaterThan),
            "gte" => Some(Self::GreaterThanOrEqual),
            "lt" => Some(Self::LessThan),
            "lte" => Some(Self::LessThanOrEqual),
            _ => None,
        }
    }

    fn sql_operator(self) -> &'static str {
        match self {
            Self::Is => "=",
            Self::NotEqual => "<>",
            Self::GreaterThan => ">",
            Self::GreaterThanOrEqual => ">=",
            Self::LessThan => "<",
            Self::LessThanOrEqual => "<=",
            Self::Contains | Self::StartsWith => "LIKE",
            Self::NotContains => "NOT LIKE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRule {
    pub field: RequestRuleField,
    pub operator: RequestRuleOperator,
    pub value: String,
}

impl RequestRule {
    pub fn from_query(value: &str) -> Option<Self> {
        let mut parts = value.splitn(3, '|');
        let field = RequestRuleField::from_query(parts.next()?)?;
        let operator = RequestRuleOperator::from_query(parts.next()?)?;
        let value = parts.next()?.trim();
        if value.is_empty() || !field.supports(operator) || !field.value_is_valid(value) {
            return None;
        }
        Some(Self {
            field,
            operator,
            value: value.to_string(),
        })
    }

    pub fn sql_operator(&self) -> &'static str {
        self.operator.sql_operator()
    }

    pub fn uses_like(&self) -> bool {
        matches!(
            self.operator,
            RequestRuleOperator::Contains | RequestRuleOperator::NotContains | RequestRuleOperator::StartsWith
        )
    }

    pub fn sql_value(&self) -> String {
        match self.operator {
            RequestRuleOperator::Contains | RequestRuleOperator::NotContains => format!("%{}%", escape_like(&self.value)),
            RequestRuleOperator::StartsWith => format!("{}%", escape_like(&self.value)),
            _ => self.value.clone(),
        }
    }

}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestSummary {
    pub id: i64,
    pub timestamp: String,
    pub duration_ms: i64,
    pub client_ip: String,
    pub client_ua: Option<String>,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub resp_status: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestPage {
    pub items: Vec<RequestSummary>,
    pub page: usize,
    pub page_size: usize,
    pub total_items: i64,
    pub total_pages: usize,
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Default, Serialize)]
pub struct DashboardStats {
    pub total_requests: i64,
    pub error_count: i64,
    pub avg_latency_ms: f64,
}
