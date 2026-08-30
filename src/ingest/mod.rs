pub mod schema;
pub mod writer;

use crate::capture::HttpEvent;
use std::path::PathBuf;
use tokio::sync::mpsc::{Sender, channel};
use tracing::warn;

pub const INGEST_CHANNEL_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct IngestSender {
    tx: Sender<HttpEvent>,
}

impl IngestSender {
    pub fn new(tx: Sender<HttpEvent>) -> Self {
        Self { tx }
    }

    pub fn send_event(&self, event: HttpEvent) {
        if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) = self.tx.try_send(event) {
            warn!("Ingest queue full (1024 items). Dropping HTTP telemetry event (fail-open).");
        }
    }
}

pub fn start_ingest_worker(db_path: PathBuf) -> (IngestSender, tokio::task::JoinHandle<()>) {
    let (tx, rx) = channel(INGEST_CHANNEL_CAPACITY);
    let handle = tokio::spawn(async move {
        writer::run_writer(db_path, rx).await;
    });
    (IngestSender::new(tx), handle)
}
