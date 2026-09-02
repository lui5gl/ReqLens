use super::model::{
    DashboardStats, RequestDetail, RequestFilter, RequestPage, RequestRule, RequestSort,
    RequestSummary,
};
use crate::error::Result;
use rusqlite::{Connection, OpenFlags, params, params_from_iter, types::Value};
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
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN resp_status >= 400 THEN 1 ELSE 0 END), 0), COALESCE(AVG(duration_ms), 0.0) FROM requests",
    )?;
    stmt.query_row([], |row| {
        Ok(DashboardStats {
            total_requests: row.get(0)?,
            error_count: row.get(1)?,
            avg_latency_ms: row.get(2)?,
        })
    })
    .map_err(Into::into)
}

pub fn fetch_request_page(
    conn: &Connection,
    filter: RequestFilter,
    sort: RequestSort,
    search: &str,
    rules: &[RequestRule],
    page: usize,
    page_size: usize,
) -> Result<RequestPage> {
    let (where_clause, values) = request_where_clause(filter, search, rules);
    let total_items = conn.query_row(
        &format!("SELECT COUNT(*) FROM requests WHERE {where_clause}"),
        params_from_iter(values.iter()),
        |row| row.get(0),
    )?;
    let total_pages = ((total_items as usize).saturating_add(page_size - 1) / page_size).max(1);
    let page = page.min(total_pages).max(1);
    let offset = (page - 1).saturating_mul(page_size);
    let sql = format!(
        "SELECT id, timestamp, duration_ms, client_ip, client_ua, method, path, query, resp_status FROM requests WHERE {where_clause} {} LIMIT ? OFFSET ?",
        sort.sql_order_by()
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut page_values = values;
    page_values.push(Value::Integer(page_size as i64));
    page_values.push(Value::Integer(offset as i64));
    let rows = stmt.query_map(params_from_iter(page_values), map_summary_row)?;
    let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(RequestPage {
        items,
        page,
        page_size,
        total_items,
        total_pages,
    })
}

fn request_where_clause(
    filter: RequestFilter,
    search: &str,
    rules: &[RequestRule],
) -> (String, Vec<Value>) {
    let mut predicates = vec![match filter {
        RequestFilter::All => "1=1".to_string(),
        RequestFilter::Errors => "resp_status >= 400".to_string(),
        RequestFilter::Slow => "duration_ms >= 500".to_string(),
    }];
    let mut values = Vec::new();
    let search = search.trim();
    if !search.is_empty() {
        predicates.push("(CAST(id AS TEXT) LIKE ? ESCAPE '\\' OR timestamp LIKE ? ESCAPE '\\' OR CAST(duration_ms AS TEXT) LIKE ? ESCAPE '\\' OR client_ip LIKE ? ESCAPE '\\' OR COALESCE(client_ua, '') LIKE ? ESCAPE '\\' OR method LIKE ? ESCAPE '\\' OR path LIKE ? ESCAPE '\\' OR COALESCE(query, '') LIKE ? ESCAPE '\\' OR CAST(resp_status AS TEXT) LIKE ? ESCAPE '\\' OR req_headers LIKE ? ESCAPE '\\' OR COALESCE(req_body, '') LIKE ? ESCAPE '\\' OR resp_headers LIKE ? ESCAPE '\\' OR COALESCE(resp_body, '') LIKE ? ESCAPE '\\')".to_string());
        let pattern = Value::Text(like_pattern(search));
        values.extend([
            pattern.clone(), pattern.clone(), pattern.clone(), pattern.clone(),
            pattern.clone(), pattern.clone(), pattern.clone(), pattern.clone(),
            pattern.clone(), pattern.clone(), pattern.clone(), pattern.clone(), pattern,
        ]);
    }
    for rule in rules {
        let escape_clause = if rule.uses_like() { " ESCAPE '\\'" } else { "" };
        predicates.push(format!(
            "{} {} ?{escape_clause}",
            rule.field.sql_column(),
            rule.sql_operator()
        ));
        values.push(rule_value(rule));
    }
    (predicates.join(" AND "), values)
}

fn rule_value(rule: &RequestRule) -> Value {
    if rule.field.is_numeric() {
        return Value::Integer(rule.value.parse().expect("validated numeric rule value"));
    }
    Value::Text(rule.sql_value())
}

pub fn fetch_request_detail(conn: &Connection, id: i64) -> Result<Option<RequestDetail>> {
    let mut stmt = conn.prepare("SELECT id, timestamp, duration_ms, client_ip, client_ua, method, path, query, req_headers, req_body, resp_status, resp_headers, resp_body FROM requests WHERE id = ?")?;
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
    rows.next().transpose().map_err(Into::into)
}

fn like_pattern(search: &str) -> String {
    format!(
        "%{}%",
        search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}

fn map_summary_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RequestSummary> {
    Ok(RequestSummary {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        duration_ms: row.get(2)?,
        client_ip: row.get(3)?,
        client_ua: row.get(4)?,
        method: row.get(5)?,
        path: row.get(6)?,
        query: row.get(7)?,
        resp_status: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::schema::initialize_schema;

    #[test]
    fn fetches_stats_requests_and_details() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        conn.execute("INSERT INTO requests (timestamp, duration_ms, client_ip, method, path, req_headers, resp_status, resp_headers) VALUES ('2026-09-01T10:00:00.000Z', 120, '127.0.0.1', 'GET', '/users', '{}', 200, '{}'), ('2026-09-01T10:01:00.000Z', 650, '127.0.0.1', 'POST', '/login', '{}', 500, '{}')", []).unwrap();

        assert_eq!(fetch_stats(&conn).unwrap().error_count, 1);
        assert_eq!(
            fetch_request_page(
                &conn,
                RequestFilter::Errors,
                RequestSort::from_query(None, None),
                "",
                &[],
                1,
                10,
            )
                .unwrap()
                .items
                .len(),
            1
        );
        assert_eq!(
            fetch_request_page(
                &conn,
                RequestFilter::All,
                RequestSort::from_query(None, None),
                "%",
                &[],
                1,
                10,
            )
                .unwrap()
                .total_items,
            0
        );
        assert_eq!(
            fetch_request_detail(&conn, 2).unwrap().unwrap().path,
            "/login"
        );
    }

    #[test]
    fn paginates_requests_in_the_repository() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        for request_number in 1..=3 {
            conn.execute(
                "INSERT INTO requests (timestamp, duration_ms, client_ip, method, path, req_headers, resp_status, resp_headers) VALUES (?1, 10, '127.0.0.1', 'GET', ?2, '{}', 200, '{}')",
                params![format!("2026-09-01T10:0{request_number}:00.000Z"), format!("/request-{request_number}")],
            )
            .unwrap();
        }

        let page = fetch_request_page(
            &conn,
            RequestFilter::All,
            RequestSort::from_query(Some("id"), Some("asc")),
            "",
            &[],
            2,
            2,
        )
            .unwrap();

        assert_eq!(page.total_items, 3);
        assert_eq!(page.total_pages, 2);
        assert_eq!(page.page, 2);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].path, "/request-3");
    }

    #[test]
    fn searches_all_request_columns_including_query_and_user_agent() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO requests (timestamp, duration_ms, client_ip, client_ua, method, path, query, req_headers, resp_status, resp_headers) VALUES (?1, 10, '127.0.0.1', ?2, 'GET', '/search', ?3, '{}', 200, '{}')",
            params!["2026-09-01T10:00:00.000Z", "ReqLens test client", "page=2&sort=recent"],
        )
        .unwrap();

        let query_page = fetch_request_page(
            &conn,
            RequestFilter::All,
            RequestSort::from_query(None, None),
            "sort=recent",
            &[],
            1,
            25,
        )
        .unwrap();
        let user_agent_page = fetch_request_page(
            &conn,
            RequestFilter::All,
            RequestSort::from_query(None, None),
            "ReqLens test client",
            &[],
            1,
            25,
        )
        .unwrap();

        assert_eq!(query_page.total_items, 1);
        assert_eq!(query_page.items[0].query.as_deref(), Some("page=2&sort=recent"));
        assert_eq!(user_agent_page.total_items, 1);
        assert_eq!(user_agent_page.items[0].client_ua.as_deref(), Some("ReqLens test client"));
    }

    #[test]
    fn combines_advanced_rules_with_bound_values() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        conn.execute("INSERT INTO requests (timestamp, duration_ms, client_ip, method, path, req_headers, resp_status, resp_headers) VALUES ('2026-09-01T10:00:00.000Z', 120, '10.0.0.5', 'GET', '/health', '{}', 200, '{}'), ('2026-09-01T10:01:00.000Z', 800, '10.0.0.6', 'POST', '/orders', '{}', 503, '{}')", []).unwrap();
        let rules = vec![
            RequestRule::from_query("status|gte|500").unwrap(),
            RequestRule::from_query("path|starts_with|/ord").unwrap(),
        ];

        let page = fetch_request_page(
            &conn,
            RequestFilter::All,
            RequestSort::from_query(None, None),
            "",
            &rules,
            1,
            25,
        )
        .unwrap();

        assert_eq!(page.total_items, 1);
        assert_eq!(page.items[0].path, "/orders");
    }

    #[test]
    fn sorts_requests_by_the_selected_column_and_direction() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        conn.execute("INSERT INTO requests (timestamp, duration_ms, client_ip, method, path, req_headers, resp_status, resp_headers) VALUES ('2026-09-01T10:00:00.000Z', 10, '10.0.0.9', 'GET', '/first', '{}', 200, '{}'), ('2026-09-01T10:01:00.000Z', 10, '10.0.0.2', 'GET', '/second', '{}', 200, '{}')", []).unwrap();

        let page = fetch_request_page(
            &conn,
            RequestFilter::All,
            RequestSort::from_query(Some("client_ip"), Some("asc")),
            "",
            &[],
            1,
            25,
        )
        .unwrap();

        assert_eq!(page.items[0].client_ip, "10.0.0.2");
        assert_eq!(page.items[1].client_ip, "10.0.0.9");
    }
}
