pub mod schema;
pub mod writer;

use crate::capture::HttpEvent;
use std::path::PathBuf;
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::thread::{JoinHandle, spawn};
use tracing::warn;

pub const INGEST_CHANNEL_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct IngestSender {
    tx: SyncSender<HttpEvent>,
}

impl IngestSender {
    pub fn new(tx: SyncSender<HttpEvent>) -> Self {
        Self { tx }
    }

    pub fn send_event(&self, event: HttpEvent) {
        if let Err(TrySendError::Full(_)) = self.tx.try_send(event) {
            warn!("Ingest queue full (1024 items). Dropping HTTP telemetry event (fail-open).");
        }
    }
}

pub fn start_ingest_worker(db_path: PathBuf) -> (IngestSender, JoinHandle<()>) {
    let (tx, rx) = sync_channel(INGEST_CHANNEL_CAPACITY);
    let handle = spawn(move || {
        writer::run_writer(db_path, rx);
    });
    (IngestSender::new(tx), handle)
}
