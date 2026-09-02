use crate::web::model::{RequestFilter, RequestRule, RequestSort};
use crate::web::repo::{fetch_request_detail, fetch_request_page, fetch_stats, open_readonly_conn};
use std::path::Path;

const MAX_REQUEST_RULES: usize = 8;
const DEFAULT_REQUEST_PAGE_SIZE: usize = 25;
const ALLOWED_REQUEST_PAGE_SIZES: [usize; 4] = [10, 25, 50, 100];

pub fn route_api(path: &str, database_path: &Path) -> ApiResponse {
    let Some(connection) = open_readonly_conn(database_path).ok().flatten() else {
        return ApiResponse::error(503, "La base de datos no esta disponible.");
    };

    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    if path == "/api/stats" {
        return fetch_stats(&connection).map_or_else(
            |error| ApiResponse::error(500, &error.to_string()),
            |stats| ApiResponse::json(200, &stats),
        );
    }

    if path == "/api/requests" {
        let filter = RequestFilter::from_query(query_value(query, "filter").as_deref());
        let sort = RequestSort::from_query(
            query_value(query, "sort").as_deref(),
            query_value(query, "direction").as_deref(),
        );
        let search = query_value(query, "search").unwrap_or_default();
        let rule_values = query_values(query, "rule");
        if rule_values.len() > MAX_REQUEST_RULES {
            return ApiResponse::error(400, "Se permiten como máximo ocho reglas de filtro.");
        }
        let Some(rules) = rule_values
            .iter()
            .map(|value| RequestRule::from_query(value))
            .collect::<Option<Vec<_>>>()
        else {
            return ApiResponse::error(400, "Una regla de filtro no es válida.");
        };
        let page = query_value(query, "page")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|page| *page > 0)
            .unwrap_or(1);
        let page_size = query_value(query, "page_size")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|page_size| ALLOWED_REQUEST_PAGE_SIZES.contains(page_size))
            .unwrap_or(DEFAULT_REQUEST_PAGE_SIZE);
        return fetch_request_page(&connection, filter, sort, &search, &rules, page, page_size).map_or_else(
            |error| ApiResponse::error(500, &error.to_string()),
            |request_page| ApiResponse::json(200, &request_page),
        );
    }

    let Some(id) = path
        .strip_prefix("/api/requests/")
        .and_then(|id| id.parse().ok())
    else {
        return ApiResponse::error(404, "Ruta API no encontrada.");
    };

    fetch_request_detail(&connection, id).map_or_else(
        |error| ApiResponse::error(500, &error.to_string()),
        |detail| match detail {
            Some(detail) => ApiResponse::json(200, &detail),
            None => ApiResponse::error(404, "Solicitud no encontrada."),
        },
    )
}

fn query_values(query: &str, key: &str) -> Vec<String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            (name == key).then(|| decode_query_value(value))
        })
        .collect()
}

fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| decode_query_value(value))
    })
}

fn decode_query_value(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut bytes = value.as_bytes().iter().copied();

    while let Some(byte) = bytes.next() {
        if byte == b'+' {
            decoded.push(' ');
            continue;
        }
        if byte != b'%' {
            decoded.push(byte as char);
            continue;
        }

        let Some(high) = bytes.next() else {
            decoded.push('%');
            break;
        };
        let Some(low) = bytes.next() else {
            decoded.push('%');
            decoded.push(high as char);
            break;
        };
        let hex = [high, low];
        let Ok(hex) = std::str::from_utf8(&hex) else {
            continue;
        };
        match u8::from_str_radix(hex, 16) {
            Ok(byte) => decoded.push(byte as char),
            Err(_) => {
                decoded.push('%');
                decoded.push(high as char);
                decoded.push(low as char);
            }
        }
    }
    decoded
}

pub struct ApiResponse {
    pub status: u16,
    pub body: String,
}

impl ApiResponse {
    fn json<T: serde::Serialize>(status: u16, value: &T) -> Self {
        Self {
            status,
            body: serde_json::to_string(value).unwrap_or_else(|_| "{}".into()),
        }
    }

    fn error(status: u16, message: &str) -> Self {
        Self::json(status, &serde_json::json!({ "error": message }))
    }
}

#[cfg(test)]
mod tests {
    use super::route_api;
    use crate::ingest::schema::initialize_schema;
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    #[test]
    fn returns_request_summary_and_detail_as_json() {
        let database = NamedTempFile::new().unwrap();
        let connection = Connection::open(database.path()).unwrap();
        initialize_schema(&connection).unwrap();
        connection.execute("INSERT INTO requests (timestamp, duration_ms, client_ip, method, path, req_headers, resp_status, resp_headers, resp_body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)", ("2026-09-01T12:00:00.000Z", 25, "127.0.0.1", "GET", "/health", "{}", 500, "{}", "failure")).unwrap();

        let list = route_api("/api/requests?filter=errors&sort=recent&page=1&page_size=10", database.path());
        let filtered_list = route_api(
            "/api/requests?sort=recent&page=1&rule=status%7Cgte%7C500",
            database.path(),
        );
        let invalid_rule = route_api(
            "/api/requests?sort=recent&page=1&rule=status%7Ccontains%7C500",
            database.path(),
        );
        let detail = route_api("/api/requests/1", database.path());
        let stats = route_api("/api/stats", database.path());

        assert_eq!(list.status, 200);
        assert!(list.body.contains("/health"));
        assert!(list.body.contains("total_pages"));
        assert!(list.body.contains("page_size\":10"));
        assert_eq!(filtered_list.status, 200);
        assert!(filtered_list.body.contains("/health"));
        assert_eq!(invalid_rule.status, 400);
        assert_eq!(detail.status, 200);
        assert!(detail.body.contains("failure"));
        assert_eq!(stats.status, 200);
        assert!(stats.body.contains("total_requests"));
    }
}
