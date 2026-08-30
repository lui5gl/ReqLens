use crate::error::Result;
use rusqlite::Connection;

pub fn initialize_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;

        CREATE TABLE IF NOT EXISTS requests (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp    TEXT    NOT NULL,
            duration_ms  INTEGER NOT NULL,
            client_ip    TEXT    NOT NULL,
            client_ua    TEXT,
            method       TEXT    NOT NULL,
            path         TEXT    NOT NULL,
            query        TEXT,
            req_headers  TEXT    NOT NULL,
            req_body     TEXT,
            resp_status  INTEGER NOT NULL,
            resp_headers TEXT    NOT NULL,
            resp_body    TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_requests_timestamp   ON requests (timestamp);
        CREATE INDEX IF NOT EXISTS idx_requests_method_path ON requests (method, path);
        CREATE INDEX IF NOT EXISTS idx_requests_resp_status ON requests (resp_status);
        CREATE INDEX IF NOT EXISTS idx_requests_client_ip   ON requests (client_ip);
        "#,
    )?;
    Ok(())
}
