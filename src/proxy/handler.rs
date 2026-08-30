use bytes::Bytes;
use chrono::Utc;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpStream;
use tracing::{error, warn};

use crate::capture::headers::serialize_headers;
use crate::capture::{HttpEvent, process_body};
use crate::config::cli::AppConfig;
use crate::ingest::IngestSender;
use crate::proxy::forward::build_upstream_request;

pub async fn handle_request(
    req: Request<Incoming>,
    client_addr: SocketAddr,
    config: Arc<AppConfig>,
    ingest: IngestSender,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let start_time = Instant::now();
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|q| q.to_string());
    let req_headers_json = serialize_headers(req.headers());
    let client_ua = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let (parts, incoming_body) = req.into_parts();
    let req_bytes = match incoming_body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            warn!("Failed to read request body: {}", e);
            Bytes::new()
        }
    };

    let req_body_str = process_body(
        &req_bytes,
        &parts.headers,
        config.max_body,
        config.redact_enabled,
    );

    let upstream_req = match build_upstream_request(
        &parts.method,
        &parts.uri,
        &parts.headers,
        req_bytes,
        &config.upstream_uri,
        client_addr,
    ) {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to build upstream request: {}", e);
            return Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Full::new(Bytes::from(
                    "Bad Gateway - Request formatting error\n",
                )))
                .unwrap());
        }
    };

    let upstream_res = forward_to_upstream(&config.upstream_uri, upstream_req).await;
    let duration_ms = start_time.elapsed().as_millis() as i64;

    match upstream_res {
        Ok((status, headers, resp_bytes)) => {
            let resp_headers_json = serialize_headers(&headers);
            let resp_body_str = process_body(
                &resp_bytes,
                &headers,
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
                resp_status: status.as_u16(),
                resp_headers: resp_headers_json,
                resp_body: resp_body_str,
            };
            ingest.send_event(event);

            let mut client_resp = Response::builder().status(status);
            for (k, v) in headers.iter() {
                client_resp = client_resp.header(k, v);
            }
            Ok(client_resp.body(Full::new(resp_bytes)).unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Full::new(Bytes::new()))
                    .unwrap()
            }))
        }
        Err(err) => {
            error!("Upstream connection error: {}", err);
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
                resp_status: StatusCode::BAD_GATEWAY.as_u16(),
                resp_headers: "{}".into(),
                resp_body: Some(format!("[PROXY ERROR: {}]", err)),
            };
            ingest.send_event(event);

            Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Full::new(Bytes::from(
                    "Bad Gateway - Upstream unavailable\n",
                )))
                .unwrap())
        }
    }
}

async fn forward_to_upstream(
    upstream_uri: &Uri,
    req: Request<Full<Bytes>>,
) -> Result<(StatusCode, hyper::HeaderMap, Bytes), String> {
    let host = upstream_uri
        .host()
        .ok_or_else(|| "Missing upstream host".to_string())?;
    let port = upstream_uri.port_u16().unwrap_or(80);
    let addr = format!("{}:{}", host, port);

    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("Connect to upstream {} failed: {}", addr, e))?;
    let io = TokioIo::new(stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .map_err(|e| format!("Upstream HTTP/1.1 handshake failed: {}", e))?;

    tokio::spawn(async move {
        if let Err(err) = conn.await {
            warn!("Upstream connection terminated with error: {:?}", err);
        }
    });

    let resp = sender
        .send_request(req)
        .await
        .map_err(|e| format!("Upstream send_request failed: {}", e))?;

    let status = resp.status();
    let headers = resp.headers().clone();
    let resp_bytes = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| format!("Failed to read upstream body: {}", e))?
        .to_bytes();

    Ok((status, headers, resp_bytes))
}
