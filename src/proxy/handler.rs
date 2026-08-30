use chrono::Utc;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, warn};

use crate::capture::headers::serialize_headers;
use crate::capture::{HttpEvent, process_body};
use crate::config::cli::AppConfig;
use crate::ingest::IngestSender;
use crate::proxy::forward::build_upstream_headers;

pub fn handle_connection(
    mut client: TcpStream,
    client_addr: SocketAddr,
    config: Arc<AppConfig>,
    ingest: IngestSender,
) {
    let start_time = Instant::now();
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let (req_raw, req_header_len) = match read_headers(&mut client) {
        Ok(res) => res,
        Err(e) => {
            warn!("Client header read error from {}: {}", client_addr, e);
            return;
        }
    };

    let mut headers_buf = [httparse::EMPTY_HEADER; 64];
    let mut parsed_req = httparse::Request::new(&mut headers_buf);

    if parsed_req.parse(&req_raw).is_err() {
        warn!("Invalid HTTP request from {}", client_addr);
        return;
    }

    let method = parsed_req.method.unwrap_or("GET").to_string();
    let raw_path = parsed_req.path.unwrap_or("/").to_string();
    let (path, query) = match raw_path.split_once('?') {
        Some((p, q)) => (p.to_string(), Some(q.to_string())),
        None => (raw_path.clone(), None),
    };

    let client_headers: Vec<(String, String)> = parsed_req
        .headers
        .iter()
        .filter_map(|h| {
            let val = std::str::from_utf8(h.value).ok()?;
            Some((h.name.to_string(), val.to_string()))
        })
        .collect();

    let client_ua = client_headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
        .map(|(_, v)| v.clone());

    let content_len = get_content_length(&client_headers);
    let is_chunked = is_chunked_encoding(&client_headers);

    let req_body_bytes = if is_chunked {
        read_chunked_body(&mut client, &req_raw[req_header_len..])
    } else if let Some(len) = content_len {
        read_fixed_body(&mut client, &req_raw[req_header_len..], len)
    } else {
        Vec::new()
    };

    let req_headers_json = serialize_headers(&client_headers);
    let req_body_str = process_body(
        &req_body_bytes,
        &client_headers,
        config.max_body,
        config.redact_enabled,
    );

    let mut upstream = match TcpStream::connect(&config.upstream_addr) {
        Ok(stream) => stream,
        Err(err) => {
            error!(
                "Upstream connection failed to {}: {}",
                config.upstream_addr, err
            );
            let duration_ms = start_time.elapsed().as_millis() as i64;
            let event = HttpEvent {
                timestamp,
                duration_ms,
                client_ip: client_addr.ip().to_string(),
                client_ua,
                method,
                path,
                query,
                req_headers: req_headers_json,
                req_body: req_body_str,
                resp_status: 502,
                resp_headers: "{}".into(),
                resp_body: Some(format!("[PROXY ERROR: Upstream {} unavailable]", err)),
            };
            ingest.send_event(event);

            let resp_502 = b"HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\nContent-Length: 35\r\nConnection: close\r\n\r\nBad Gateway - Upstream unavailable\n";
            let _ = client.write_all(resp_502);
            let _ = client.flush();
            return;
        }
    };

    let _ = upstream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = upstream.set_write_timeout(Some(Duration::from_secs(30)));

    let forward_headers =
        build_upstream_headers(&client_headers, &config.upstream_host, client_addr);

    let mut upstream_req_bytes = Vec::with_capacity(1024 + req_body_bytes.len());
    upstream_req_bytes
        .extend_from_slice(format!("{} {} HTTP/1.1\r\n", method, raw_path).as_bytes());

    for (k, v) in &forward_headers {
        upstream_req_bytes.extend_from_slice(format!("{}: {}\r\n", k, v).as_bytes());
    }
    if content_len.is_some() && !is_chunked {
        upstream_req_bytes
            .extend_from_slice(format!("Content-Length: {}\r\n", req_body_bytes.len()).as_bytes());
    }
    upstream_req_bytes.extend_from_slice(b"\r\n");
    upstream_req_bytes.extend_from_slice(&req_body_bytes);

    if let Err(e) = upstream.write_all(&upstream_req_bytes) {
        warn!("Failed to write request to upstream: {}", e);
        return;
    }
    let _ = upstream.flush();

    let (resp_raw, resp_header_len) = match read_headers(&mut upstream) {
        Ok(res) => res,
        Err(e) => {
            warn!("Failed to read headers from upstream: {}", e);
            return;
        }
    };

    let mut resp_headers_buf = [httparse::EMPTY_HEADER; 64];
    let mut parsed_resp = httparse::Response::new(&mut resp_headers_buf);

    if parsed_resp.parse(&resp_raw).is_err() {
        warn!("Invalid HTTP response from upstream");
        return;
    }

    let status_code = parsed_resp.code.unwrap_or(200);

    let resp_headers: Vec<(String, String)> = parsed_resp
        .headers
        .iter()
        .filter_map(|h| {
            let val = std::str::from_utf8(h.value).ok()?;
            Some((h.name.to_string(), val.to_string()))
        })
        .collect();

    let resp_content_len = get_content_length(&resp_headers);
    let resp_is_chunked = is_chunked_encoding(&resp_headers);

    let resp_body_bytes = if resp_is_chunked {
        read_chunked_body(&mut upstream, &resp_raw[resp_header_len..])
    } else if let Some(len) = resp_content_len {
        read_fixed_body(&mut upstream, &resp_raw[resp_header_len..], len)
    } else {
        read_until_eof(&mut upstream, &resp_raw[resp_header_len..])
    };

    let _ = client.write_all(&resp_raw[..resp_header_len]);
    let _ = client.write_all(&resp_body_bytes);
    let _ = client.flush();

    let duration_ms = start_time.elapsed().as_millis() as i64;
    let resp_headers_json = serialize_headers(&resp_headers);
    let resp_body_str = process_body(
        &resp_body_bytes,
        &resp_headers,
        config.max_body,
        config.redact_enabled,
    );

    let event = HttpEvent {
        timestamp,
        duration_ms,
        client_ip: client_addr.ip().to_string(),
        client_ua,
        method,
        path,
        query,
        req_headers: req_headers_json,
        req_body: req_body_str,
        resp_status: status_code,
        resp_headers: resp_headers_json,
        resp_body: resp_body_str,
    };
    ingest.send_event(event);
}

fn read_headers(stream: &mut TcpStream) -> std::io::Result<(Vec<u8>, usize)> {
    let mut buf = Vec::with_capacity(4096);
    let mut temp = [0u8; 1024];

    loop {
        let n = stream.read(&mut temp)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&temp[..n]);

        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            return Ok((buf, pos + 4));
        }
        if buf.len() > 65536 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "HTTP headers exceed 64 KB",
            ));
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "Connection closed before headers completed",
    ))
}

fn get_content_length(headers: &[(String, String)]) -> Option<usize> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse().ok())
}

fn is_chunked_encoding(headers: &[(String, String)]) -> bool {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("transfer-encoding"))
        .map(|(_, v)| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false)
}

fn read_fixed_body(stream: &mut TcpStream, initial: &[u8], len: usize) -> Vec<u8> {
    let mut body = Vec::with_capacity(len);
    body.extend_from_slice(initial);

    let mut temp = [0u8; 4096];
    while body.len() < len {
        let needed = (len - body.len()).min(temp.len());
        match stream.read(&mut temp[..needed]) {
            Ok(0) => break,
            Ok(n) => body.extend_from_slice(&temp[..n]),
            Err(_) => break,
        }
    }
    body
}

fn read_chunked_body(stream: &mut TcpStream, initial: &[u8]) -> Vec<u8> {
    let mut raw = Vec::from(initial);
    let mut temp = [0u8; 4096];

    while !raw.windows(5).any(|w| w == b"0\r\n\r\n") {
        match stream.read(&mut temp) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&temp[..n]),
            Err(_) => break,
        }
    }
    raw
}

fn read_until_eof(stream: &mut TcpStream, initial: &[u8]) -> Vec<u8> {
    let mut body = Vec::from(initial);
    let mut temp = [0u8; 4096];
    while let Ok(n) = stream.read(&mut temp) {
        if n == 0 {
            break;
        }
        body.extend_from_slice(&temp[..n]);
    }
    body
}
