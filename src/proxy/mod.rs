pub mod forward;
pub mod handler;

use crate::config::cli::AppConfig;
use crate::error::Result;
use crate::ingest::IngestSender;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use tracing::{error, info};

const MAX_CONCURRENT_CONNECTIONS: usize = 256;

struct ConnectionPermit {
    active_connections: Arc<AtomicUsize>,
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.active_connections.fetch_sub(1, Ordering::Release);
    }
}

pub fn run_server(
    config: Arc<AppConfig>,
    ingest: IngestSender,
    running: Arc<AtomicBool>,
) -> Result<()> {
    let listener = TcpListener::bind(config.listen_addr)?;
    info!("ReqLens listening on http://{}", config.listen_addr);
    info!(
        "Forwarding upstream traffic to http://{}",
        config.upstream_addr
    );
    // A non-blocking listener lets us observe the shutdown flag without relying
    // on poll(2). Some older Linux hosts return immediately from poll with an
    // error, which otherwise turns this loop into a CPU-burning busy spin.
    listener.set_nonblocking(true)?;
    let active_connections = Arc::new(AtomicUsize::new(0));

    while running.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, client_addr)) => {
                let active_connections = Arc::clone(&active_connections);
                if active_connections
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                        (active < MAX_CONCURRENT_CONNECTIONS).then_some(active + 1)
                    })
                    .is_err()
                {
                    error!(
                        "Rejecting connection from {}: concurrent connection limit reached",
                        client_addr
                    );
                    continue;
                }

                let cfg = Arc::clone(&config);
                let ing = ingest.clone();
                thread::spawn(move || {
                    let _permit = ConnectionPermit { active_connections };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
                    handler::handle_connection(stream, client_addr, cfg, ing);
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                if running.load(Ordering::Relaxed) {
                    error!("Accept error on listener: {}", e);
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }

    info!("Proxy listener stopped.");
    Ok(())
}
