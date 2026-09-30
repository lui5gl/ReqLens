use super::model::{
    DashboardStats, FilterTab, RequestDetail, RequestFilters, RequestSummary, SortField,
};
use crate::error::Result;
use rusqlite::{Connection, OpenFlags, params, types::Value};
use std::path::Path;

pub fn open_readonly_conn(db_path: &Path) -> Result<Option<Connection>> {
    if !db_path.exists() {
        return Ok(None);
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(db_path, flags)?;
    conn.busy_timeout(std::time::Duration::from_millis(500))?;
    Ok(Some(conn))
}

pub fn fetch_stats(conn: &Connection) -> Result<DashboardStats> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            COUNT(*),
            COALESCE(SUM(CASE WHEN resp_status >= 400 THEN 1 ELSE 0 END), 0),
            COALESCE(AVG(duration_ms), 0.0)
        FROM requests
        "#,
    )?;

    let stats = stmt.query_row([], |row| {
        Ok(DashboardStats {
            total_requests: row.get(0)?,
            error_count: row.get(1)?,
            avg_latency_ms: row.get(2)?,
        })
    })?;

    Ok(stats)
}

pub fn fetch_requests(
    conn: &Connection,
    filter: FilterTab,
    sort: SortField,
    search: &str,
    filters: &RequestFilters,
    limit: usize,
) -> Result<Vec<RequestSummary>> {
    let mut conditions = Vec::new();
    let mut values = Vec::new();
    match filter {
        FilterTab::All => {}
        FilterTab::Errors => conditions.push("resp_status >= 400".to_string()),
        FilterTab::Slow => conditions.push("duration_ms >= 500".to_string()),
    }
    if filters.method != super::model::MethodFilter::Any {
        conditions.push(format!("method = ?{}", values.len() + 1));
        values.push(Value::Text(filters.method.label().to_string()));
    }
    match filters.status {
        super::model::StatusFilter::Any => {}
        super::model::StatusFilter::Errors => conditions.push("resp_status >= 400".to_string()),
        super::model::StatusFilter::ClientErrors => {
            conditions.push("resp_status BETWEEN 400 AND 499".to_string())
        }
        super::model::StatusFilter::ServerErrors => {
            conditions.push("resp_status BETWEEN 500 AND 599".to_string())
        }
    }
    if !filters.path.trim().is_empty() {
        conditions.push(format!("path LIKE ?{}", values.len() + 1));
        values.push(Value::Text(format!("%{}%", filters.path.trim())));
    }
    if let Some(min_duration) = filters.min_duration_ms {
        conditions.push(format!("duration_ms >= ?{}", values.len() + 1));
        values.push(Value::Integer(min_duration));
    }
    if !search.trim().is_empty() {
        let parameter = values.len() + 1;
        conditions.push(format!(
            "(path LIKE ?{parameter} OR method LIKE ?{parameter} OR client_ip LIKE ?{parameter} OR CAST(resp_status AS TEXT) LIKE ?{parameter})"
        ));
        values.push(Value::Text(format!("%{}%", search.trim())));
    }
    values.push(Value::Integer(limit as i64));
    let where_clause = if conditions.is_empty() {
        "1=1".to_string()
    } else {
        conditions.join(" AND ")
    };
    let sql = format!(
        "SELECT id, timestamp, duration_ms, client_ip, method, path, resp_status FROM requests WHERE {} {} LIMIT ?{}",
        where_clause,
        sort.sql_order_by(),
        values.len()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), map_summary_row)?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

fn map_summary_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RequestSummary> {
    Ok(RequestSummary {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        duration_ms: row.get(2)?,
        client_ip: row.get(3)?,
        method: row.get(4)?,
        path: row.get(5)?,
        resp_status: row.get(6)?,
    })
}

pub fn fetch_request_detail(conn: &Connection, id: i64) -> Result<Option<RequestDetail>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            id, timestamp, duration_ms, client_ip, client_ua,
            method, path, query, req_headers, req_body,
            resp_status, resp_headers, resp_body
        FROM requests
        WHERE id = ?
        "#,
    )?;

    let mut rows = stmt.query_map(params![id], |row| {
        Ok(RequestDetail {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            duration_ms: row.get(2)?,
            client_ip: row.get(3)?,
            client_ua: row.get(4)?,
            method: row.get(5)?,
            path: row.get(6)?,
            query: row.get(7)?,
            req_headers: row.get(8)?,
            req_body: row.get(9)?,
            resp_status: row.get(10)?,
            resp_headers: row.get(11)?,
            resp_body: row.get(12)?,
        })
    })?;

    if let Some(res) = rows.next() {
        Ok(Some(res?))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
#[path = "repo_test.rs"]
mod repo_test;
