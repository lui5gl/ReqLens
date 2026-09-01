//! Passive HTTP/1.x observation for Linux.
//!
//! The sniffer receives copies of IPv4/TCP packets through `AF_PACKET`; it
//! never binds the observed TCP port and never forwards or modifies traffic.

mod engine;
mod packet;

#[cfg(target_os = "linux")]
mod socket;

pub use engine::SniffEngine;
pub use packet::TcpSegment;

use crate::error::{ReqLensError, Result};
use crate::ingest::IngestSender;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tracing::info;

const IDLE_CAPTURE_BACKOFF: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub struct SniffConfig {
    pub interface: String,
    pub server_ip: Option<Ipv4Addr>,
    pub port: u16,
    pub max_body: usize,
    pub redact_enabled: bool,
}

#[cfg(target_os = "linux")]
pub fn run_sniffer(
    config: SniffConfig,
    ingest: IngestSender,
    running: Arc<AtomicBool>,
) -> Result<()> {
    let receive_timeout = Duration::from_millis(250);
    let capture = socket::PacketSocket::open(&config.interface, receive_timeout).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            ReqLensError::Config(
                "passive capture needs root or CAP_NET_RAW; run as root or apply: setcap cap_net_raw=eip /usr/local/bin/reqlens".into(),
            )
        } else {
            error.into()
        }
    })?;
    let mut engine = SniffEngine::new(
        config.server_ip,
        config.port,
        config.max_body,
        config.redact_enabled,
    );
    let mut buffer = vec![0_u8; 65_536];
    let mut last_cleanup = Instant::now();

    info!(
        "Passive HTTP capture active on interface '{}' for port {} (Apache traffic is not modified)",
        config.interface, config.port
    );

    while running.load(Ordering::Relaxed) {
        match capture.receive(&mut buffer) {
            Ok(Some(size)) => {
                if let Some(segment) = packet::parse_ipv4_tcp(&buffer[..size], config.port) {
                    for event in engine.process(segment) {
                        ingest.send_event(event);
                    }
                }
            }
            Ok(None) => std::thread::sleep(IDLE_CAPTURE_BACKOFF),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }

        if last_cleanup.elapsed() >= Duration::from_secs(1) {
            engine.expire_idle(Duration::from_secs(60));
            last_cleanup = Instant::now();
        }
    }

    info!("Passive HTTP capture stopped.");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn run_sniffer(
    _config: SniffConfig,
    _ingest: IngestSender,
    _running: Arc<AtomicBool>,
) -> Result<()> {
    Err(ReqLensError::Config(
        "passive capture is only supported on Linux".into(),
    ))
}
