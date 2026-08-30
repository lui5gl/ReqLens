pub mod forward;
pub mod handler;

use crate::config::cli::AppConfig;
use crate::error::Result;
use crate::ingest::IngestSender;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use tracing::{error, info};

pub fn run_server(
    config: Arc<AppConfig>,
    ingest: IngestSender,
    running: Arc<AtomicBool>,
) -> Result<()> {
    let listener = TcpListener::bind(config.listen_addr)?;
    listener.set_nonblocking(true)?;
    info!("ReqLens listening on http://{}", config.listen_addr);
    info!(
        "Forwarding upstream traffic to http://{}",
        config.upstream_addr
    );

    while running.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, client_addr)) => {
                let cfg = Arc::clone(&config);
                let ing = ingest.clone();
                thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
                    handler::handle_connection(stream, client_addr, cfg, ing);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                error!("Accept error on listener: {}", e);
            }
        }
    }

    info!("Proxy listener stopped.");
    Ok(())
}
