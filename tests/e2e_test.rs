use rusqlite::Connection;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use tempfile::NamedTempFile;

use reqlens::config::cli::AppConfig;
use reqlens::ingest;
use reqlens::proxy;

#[test]
fn test_e2e_proxy_and_telemetry_capture() {
    let upstream_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    let upstream_running = Arc::new(AtomicBool::new(true));
    let up_r = Arc::clone(&upstream_running);

    let upstream_thread = thread::spawn(move || {
        upstream_listener.set_nonblocking(true).unwrap();
        while up_r.load(Ordering::Relaxed) {
            match upstream_listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf);
                    let resp = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 32\r\nConnection: close\r\n\r\n{\"status\":\"created\",\"user_id\":42}";
                    let _ = stream.write_all(resp);
                    let _ = stream.flush();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });

    let proxy_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let temp_db = NamedTempFile::new().unwrap();
    let db_path = temp_db.path().to_path_buf();

    let config = Arc::new(AppConfig {
        listen_addr: proxy_addr,
        upstream_addr: format!("{}", upstream_addr),
        upstream_host: "127.0.0.1".to_string(),
        db_path: db_path.clone(),
        max_body: 65536,
        redact_enabled: true,
    });

    let (ingest_sender, ingest_handle) = ingest::start_ingest_worker(db_path.clone());
    let proxy_running = Arc::new(AtomicBool::new(true));

    let proxy_config = Arc::clone(&config);
    let proxy_ingest = ingest_sender.clone();
    let proxy_r = Arc::clone(&proxy_running);

    let proxy_handle = thread::spawn(move || {
        proxy::run_server(proxy_config, proxy_ingest, proxy_r).unwrap();
    });

    thread::sleep(Duration::from_millis(100));

    let mut client_stream = TcpStream::connect(proxy_addr).expect("Connect to proxy failed");
    let req_payload = "POST /api/v1/users HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer secret-token-should-not-persist\r\nContent-Length: 48\r\n\r\n{\"username\":\"testuser\",\"password\":\"mypassword123\"}";
    client_stream.write_all(req_payload.as_bytes()).unwrap();
    client_stream.flush().unwrap();

    let mut client_resp = Vec::new();
    client_stream.read_to_end(&mut client_resp).unwrap();
    let resp_str = String::from_utf8_lossy(&client_resp);

    assert!(resp_str.contains("200 OK"));
    assert!(resp_str.contains(r#"{"status":"created","user_id":42}"#));

    proxy_running.store(false, Ordering::Relaxed);
    let _ = proxy_handle.join();

    upstream_running.store(false, Ordering::Relaxed);
    let _ = upstream_thread.join();

    drop(ingest_sender);
    let _ = ingest_handle.join();

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
