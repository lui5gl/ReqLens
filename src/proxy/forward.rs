use crate::error::{ReqLensError, Result};
use bytes::Bytes;
use http_body_util::Full;
use hyper::header::{HOST, HeaderMap, HeaderValue};
use hyper::{Method, Request, Uri};
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

pub fn build_upstream_request(
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    body_bytes: Bytes,
    upstream_uri: &Uri,
    client_addr: SocketAddr,
) -> Result<Request<Full<Bytes>>> {
    let mut builder = Request::builder()
        .method(method)
        .uri(construct_target_uri(uri, upstream_uri)?);

    if let Some(headers_mut) = builder.headers_mut() {
        copy_and_filter_headers(headers, headers_mut, upstream_uri, client_addr);
    }

    builder
        .body(Full::new(body_bytes))
        .map_err(|e| ReqLensError::Upstream(e.to_string()))
}

fn construct_target_uri(req_uri: &Uri, upstream_uri: &Uri) -> Result<Uri> {
    let path_and_query = req_uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let target_str = format!("{}{}", upstream_uri, path_and_query.trim_start_matches('/'));
    target_str
        .parse()
        .map_err(|e| ReqLensError::Upstream(format!("Invalid target URI: {}", e)))
}

fn copy_and_filter_headers(
    source: &HeaderMap,
    dest: &mut HeaderMap,
    upstream: &Uri,
    client_addr: SocketAddr,
) {
    for (name, val) in source.iter() {
        let name_str = name.as_str().to_ascii_lowercase();
        if !HOP_BY_HOP_HEADERS.contains(&name_str.as_str()) && name_str != "host" {
            dest.insert(name.clone(), val.clone());
        }
    }

    if let Some(host) = upstream.host()
        && let Ok(host_val) = HeaderValue::from_str(host)
    {
        dest.insert(HOST, host_val);
    }

    append_x_forwarded_for(source, dest, client_addr);
}

fn append_x_forwarded_for(source: &HeaderMap, dest: &mut HeaderMap, client_addr: SocketAddr) {
    let client_ip = client_addr.ip().to_string();
    let xff = match source.get("x-forwarded-for") {
        Some(existing) => match existing.to_str() {
            Ok(prev) => format!("{}, {}", prev, client_ip),
            Err(_) => client_ip,
        },
        None => client_ip,
    };

    if let Ok(val) = HeaderValue::from_str(&xff) {
        dest.insert("x-forwarded-for", val);
    }
}
