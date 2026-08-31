use super::model::RequestDetail;

const EMPTY_BODY_LABEL: &str = "(vacio)";
const REQUEST_HEADERS_LABEL: &str = "--- Request Headers (Permitidos) ---";
const REQUEST_BODY_LABEL: &str = "--- Request Body (Con Redaccion Fail-Safe) ---";
const RESPONSE_HEADERS_LABEL: &str = "--- Response Headers ---";
const RESPONSE_BODY_LABEL: &str = "--- Response Body ---";

pub fn format_request_detail(detail: &RequestDetail) -> String {
    format!(
        "Request #{id}\nMethod: {method}\nPath: {path}\nStatus: {status}\nLatency: {duration} ms\nTimestamp: {timestamp}\nClient IP: {client_ip}\nUser-Agent: {client_ua}\n\n{request_headers}\n{req_headers}\n\n{request_body}\n{req_body}\n\n{response_headers}\n{resp_headers}\n\n{response_body}\n{resp_body}\n",
        id = detail.id,
        method = detail.method,
        path = detail.path,
        status = detail.resp_status,
        duration = detail.duration_ms,
        timestamp = detail.timestamp,
        client_ip = detail.client_ip,
        client_ua = detail.client_ua.as_deref().unwrap_or("-"),
        request_headers = REQUEST_HEADERS_LABEL,
        req_headers = detail.req_headers,
        request_body = REQUEST_BODY_LABEL,
        req_body = detail.req_body.as_deref().unwrap_or(EMPTY_BODY_LABEL),
        response_headers = RESPONSE_HEADERS_LABEL,
        resp_headers = detail.resp_headers,
        response_body = RESPONSE_BODY_LABEL,
        resp_body = detail.resp_body.as_deref().unwrap_or(EMPTY_BODY_LABEL),
    )
}

#[cfg(test)]
mod tests {
    use super::format_request_detail;
    use crate::tui::model::RequestDetail;

    #[test]
    fn includes_request_and_response_content() {
        let detail = RequestDetail {
            id: 12,
            timestamp: "2026-08-31T12:00:00Z".into(),
            duration_ms: 14,
            client_ip: "172.23.25.76".into(),
            client_ua: None,
            method: "GET".into(),
            path: "/health".into(),
            query: None,
            req_headers: "Host: test".into(),
            req_body: None,
            resp_status: 200,
            resp_headers: "Content-Type: text/plain".into(),
            resp_body: Some("ok".into()),
        };

        let content = format_request_detail(&detail);

        assert!(content.contains("Request #12"));
        assert!(content.contains("Host: test"));
        assert!(content.contains("Content-Type: text/plain"));
        assert!(content.ends_with("ok\n"));
    }
}
