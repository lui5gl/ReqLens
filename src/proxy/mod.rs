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

#[cfg(unix)]
fn poll_ready(listener: &TcpListener, timeout_ms: libc::c_int) -> bool {
    use std::os::unix::io::AsRawFd;
    let fd = listener.as_raw_fd();
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ret = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    ret > 0 && (pfd.revents & libc::POLLIN != 0)
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

    while running.load(Ordering::Relaxed) {
        #[cfg(unix)]
        {
            if !poll_ready(&listener, 250) {
                continue;
            }
        }

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
