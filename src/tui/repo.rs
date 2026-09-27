use super::model::{DashboardStats, FilterTab, RequestDetail, RequestSummary, SortField};
use crate::error::Result;
use rusqlite::{Connection, OpenFlags, params};
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
    limit: usize,
) -> Result<Vec<RequestSummary>> {
    let base_filter = match filter {
        FilterTab::All => "1=1",
        FilterTab::Errors => "resp_status >= 400",
        FilterTab::Slow => "duration_ms >= 500",
    };

    let trimmed_search = search.trim();
    let has_search = !trimmed_search.is_empty();

    let sql = if has_search {
        format!(
            "SELECT id, timestamp, duration_ms, client_ip, method, path, resp_status FROM requests WHERE {} AND (path LIKE ?1 OR method LIKE ?1 OR client_ip LIKE ?1 OR CAST(resp_status AS TEXT) LIKE ?1) {} LIMIT ?2",
            base_filter,
            sort.sql_order_by()
        )
    } else {
        format!(
            "SELECT id, timestamp, duration_ms, client_ip, method, path, resp_status FROM requests WHERE {} {} LIMIT ?1",
            base_filter,
            sort.sql_order_by()
        )
    };

    let mut stmt = conn.prepare(&sql)?;

    let rows = if has_search {
        let pattern = format!("%{}%", trimmed_search);
        stmt.query_map(params![pattern, limit as i64], map_summary_row)?
    } else {
        stmt.query_map(params![limit as i64], map_summary_row)?
    };

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
