use std::net::SocketAddr;

const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
];

pub fn build_upstream_headers(
    headers: &[(String, String)],
    upstream_host: &str,
    client_addr: SocketAddr,
) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(headers.len() + 3);
    let mut existing_xff: Option<String> = None;

    for (k, v) in headers {
        let k_lower = k.to_ascii_lowercase();
        if k_lower == "host" || HOP_BY_HOP_HEADERS.contains(&k_lower.as_str()) {
            continue;
        }
        if k_lower == "x-forwarded-for" {
            existing_xff = Some(v.clone());
            continue;
        }
        out.push((k.clone(), v.clone()));
    }

    out.push(("Host".to_string(), upstream_host.to_string()));

    let client_ip = client_addr.ip().to_string();
    let xff = match existing_xff {
        Some(prev) => format!("{}, {}", prev, client_ip),
        None => client_ip,
    };
    out.push(("X-Forwarded-For".to_string(), xff));
    out.push(("Connection".to_string(), "close".to_string()));

    out
}
