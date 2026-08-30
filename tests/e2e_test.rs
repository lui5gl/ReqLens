use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;

use http_body_util::Full;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use rusqlite::Connection;
use tempfile::NamedTempFile;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use reqlens::config::cli::AppConfig;
use reqlens::ingest;
use reqlens::proxy;

#[tokio::test]
async fn test_e2e_proxy_and_telemetry_capture() {
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    let (upstream_shutdown_tx, mut upstream_shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut upstream_shutdown_rx => break,
                res = upstream_listener.accept() => {
                    if let Ok((stream, _)) = res {
                        let io = TokioIo::new(stream);
                        tokio::spawn(async move {
                            let service = service_fn(|_req: Request<hyper::body::Incoming>| async {
                                Ok::<_, std::convert::Infallible>(
                                    Response::builder()
                                        .status(StatusCode::OK)
                                        .header("Content-Type", "application/json")
                                        .body(Full::new(Bytes::from(r#"{"status":"created","user_id":42}"#)))
                                        .unwrap(),
                                )
                            });
                            let _ = hyper::server::conn::http1::Builder::new().serve_connection(io, service).await;
                        });
                    }
                }
            }
        }
    });

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let temp_db = NamedTempFile::new().unwrap();
    let db_path = temp_db.path().to_path_buf();

    let config = Arc::new(AppConfig {
        listen_addr: proxy_addr,
        upstream_uri: format!("http://{}", upstream_addr).parse().unwrap(),
        db_path: db_path.clone(),
        max_body: 65536,
        redact_enabled: true,
        tui_enabled: false,
    });

    let (ingest_sender, ingest_handle) = ingest::start_ingest_worker(db_path.clone());
    let (proxy_shutdown_tx, proxy_shutdown_rx) = oneshot::channel();

    let proxy_config = Arc::clone(&config);
    let proxy_ingest = ingest_sender.clone();
    let proxy_task = tokio::spawn(async move {
        proxy::run_server(proxy_config, proxy_ingest, async move {
            let _ = proxy_shutdown_rx.await;
        })
        .await
        .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{}/api/v1/users", proxy_addr))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer secret-token-should-not-persist")
        .body(r#"{"username":"testuser","password":"mypassword123"}"#)
        .send()
        .await
        .expect("Request failed");

    assert_eq!(resp.status(), 200);
    let resp_json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(resp_json["status"], "created");

    let _ = proxy_shutdown_tx.send(());
    let _ = proxy_task.await;
    let _ = upstream_shutdown_tx.send(());

    drop(ingest_sender);
    let _ = tokio::time::timeout(Duration::from_secs(3), ingest_handle).await;

    let conn = Connection::open(&db_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT method, path, req_body, resp_status, req_headers FROM requests LIMIT 1")
        .unwrap();

    let mut rows = stmt.query([]).unwrap();
    let row = rows.next().unwrap().expect("Must have 1 recorded event");

    let method: String = row.get(0).unwrap();
    let path: String = row.get(1).unwrap();
    let req_body: String = row.get(2).unwrap();
    let resp_status: u16 = row.get(3).unwrap();
    let req_headers: String = row.get(4).unwrap();

    assert_eq!(method, "POST");
    assert_eq!(path, "/api/v1/users");
    assert_eq!(resp_status, 200);
    assert!(req_body.contains(r#""password":"[REDACTED]""#));
    assert!(!req_body.contains("mypassword123"));
    assert!(!req_headers.contains("secret-token-should-not-persist"));
}
