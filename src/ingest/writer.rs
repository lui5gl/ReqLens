use crate::capture::HttpEvent;
use crate::error::Result;
use crate::ingest::schema::initialize_schema;
use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;
use tracing::{error, info};

pub const BATCH_SIZE: usize = 100;
pub const FLUSH_INTERVAL_MS: u64 = 250;

pub fn run_writer(db_path: PathBuf, rx: Receiver<HttpEvent>) {
    let mut conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to open SQLite database at {:?}: {}", db_path, e);
            return;
        }
    };

    if let Err(e) = initialize_schema(&conn) {
        error!("Failed to initialize database schema: {}", e);
        return;
    }
    info!("SQLite database initialized in WAL mode at {:?}", db_path);

    let mut buffer = Vec::with_capacity(BATCH_SIZE);
    let flush_timeout = Duration::from_millis(FLUSH_INTERVAL_MS);

    loop {
        match rx.recv_timeout(flush_timeout) {
            Ok(ev) => {
                buffer.push(ev);
                if buffer.len() >= BATCH_SIZE {
                    flush_batch(&mut conn, &mut buffer);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if !buffer.is_empty() {
                    flush_batch(&mut conn, &mut buffer);
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                if !buffer.is_empty() {
                    flush_batch(&mut conn, &mut buffer);
                }
                info!("Ingest channel closed, writer stopped.");
                break;
            }
        }
    }
}

fn flush_batch(conn: &mut Connection, buffer: &mut Vec<HttpEvent>) {
    if let Err(e) = persist_events(conn, buffer) {
        error!("Failed to commit batch of {} events: {}", buffer.len(), e);
    }
    buffer.clear();
}

fn persist_events(conn: &mut Connection, events: &[HttpEvent]) -> Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            r#"
            INSERT INTO requests (
                timestamp, duration_ms, client_ip, client_ua, method,
                path, query, req_headers, req_body, resp_status,
                resp_headers, resp_body
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )?;

        for ev in events {
            stmt.execute(params![
                ev.timestamp,
                ev.duration_ms,
                ev.client_ip,
                ev.client_ua,
                ev.method,
                ev.path,
                ev.query,
                ev.req_headers,
                ev.req_body,
                ev.resp_status,
                ev.resp_headers,
                ev.resp_body,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}
