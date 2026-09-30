use super::*;
use crate::ingest::schema::initialize_schema;
use crate::tui::model::{MethodFilter, StatusFilter};

#[test]
fn test_fetch_stats_and_requests() {
    let conn = Connection::open_in_memory().unwrap();
    initialize_schema(&conn).unwrap();

    conn.execute(
        r#"
        INSERT INTO requests (
            timestamp, duration_ms, client_ip, client_ua, method,
            path, query, req_headers, req_body, resp_status,
            resp_headers, resp_body
        ) VALUES ('2026-08-30T10:00:00.000Z', 120, '127.0.0.1', 'curl', 'GET', '/api/users', NULL, '{}', NULL, 200, '{}', '{"ok":true}'),
                 ('2026-08-30T10:01:00.000Z', 650, '127.0.0.1', 'curl', 'POST', '/api/login', NULL, '{}', '{"user":"test"}', 500, '{}', '{"error":"internal"}');
        "#,
        [],
    )
    .unwrap();

    let stats = fetch_stats(&conn).unwrap();
    assert_eq!(stats.total_requests, 2);
    assert_eq!(stats.error_count, 1);
    assert!((stats.avg_latency_ms - 385.0).abs() < f64::EPSILON);

    let filters = RequestFilters::default();
    let all_reqs =
        fetch_requests(&conn, FilterTab::All, SortField::Recent, "", &filters, 10).unwrap();
    assert_eq!(all_reqs.len(), 2);

    let error_reqs = fetch_requests(
        &conn,
        FilterTab::Errors,
        SortField::Recent,
        "",
        &filters,
        10,
    )
    .unwrap();
    assert_eq!(error_reqs.len(), 1);
    assert_eq!(error_reqs[0].resp_status, 500);

    let slow_reqs =
        fetch_requests(&conn, FilterTab::Slow, SortField::Slowest, "", &filters, 10).unwrap();
    assert_eq!(slow_reqs.len(), 1);
    assert_eq!(slow_reqs[0].duration_ms, 650);

    let search_reqs = fetch_requests(
        &conn,
        FilterTab::All,
        SortField::Recent,
        "login",
        &filters,
        10,
    )
    .unwrap();
    assert_eq!(search_reqs.len(), 1);
    assert_eq!(search_reqs[0].path, "/api/login");

    let combined_filters = RequestFilters {
        method: MethodFilter::Post,
        status: StatusFilter::ServerErrors,
        path: "/api".to_string(),
        min_duration_ms: Some(600),
    };
    let combined_reqs = fetch_requests(
        &conn,
        FilterTab::All,
        SortField::Recent,
        "",
        &combined_filters,
        10,
    )
    .unwrap();
    assert_eq!(combined_reqs.len(), 1);
    assert_eq!(combined_reqs[0].path, "/api/login");

    let detail = fetch_request_detail(&conn, error_reqs[0].id)
        .unwrap()
        .unwrap();
    assert_eq!(detail.method, "POST");
    assert_eq!(detail.path, "/api/login");
    assert_eq!(detail.resp_status, 500);
}
