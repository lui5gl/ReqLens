use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use reqlens::config::cli::Commands;
use reqlens::config::{self, parse_cli, parse_upstream};
use reqlens::ingest;
use reqlens::ops;
use reqlens::proxy;
use reqlens::tui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_cli();

    match args.command {
        Some(Commands::Status { db_path }) => {
            ops::print_status(&db_path)?;
            return Ok(());
        }
        Some(Commands::Restart) => {
            ops::restart_service()?;
            return Ok(());
        }
        Some(Commands::Disable) => {
            ops::disable_service()?;
            return Ok(());
        }
        Some(Commands::Install {
            listen,
            upstream,
            db_path,
            max_body,
            no_redact,
        }) => {
            ops::install_service(&listen, &upstream, &db_path, max_body, no_redact)?;
            return Ok(());
        }
        Some(Commands::Uninstall { purge }) => {
            ops::uninstall_service(purge)?;
            return Ok(());
        }
        Some(Commands::Tui {
            db_path,
            listen,
            upstream,
        }) => {
            let (upstream_addr, upstream_host) = parse_upstream(&upstream)?;
            let tui_cfg = reqlens::config::cli::AppConfig {
                listen_addr: listen
                    .parse()
                    .unwrap_or_else(|_| "0.0.0.0:8080".parse().unwrap()),
                upstream_addr,
                upstream_host,
                db_path,
                max_body: 65536,
                redact_enabled: true,
                tui_enabled: true,
            };
            tui::run_tui_app(&tui_cfg)?;
            return Ok(());
        }
        _ => {}
    }

    let config = config::load_config()?;

    if !config.tui_enabled {
        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    if !config.redact_enabled {
        warn!(
            "⚠️ SECRETS REDACTION IS DISABLED (--no-redact). Sensitive data will be written to disk!"
        );
    }

    let (ingest_sender, ingest_handle) = ingest::start_ingest_worker(config.db_path.clone());
    let config = Arc::new(config);
    let running = Arc::new(AtomicBool::new(true));

    let r_clone = Arc::clone(&running);
    let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&r_clone));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&r_clone));

    if config.tui_enabled {
        let proxy_cfg = Arc::clone(&config);
        let proxy_ingest = ingest_sender.clone();
        let proxy_running = Arc::clone(&running);

        let proxy_handle = thread::spawn(move || {
            let _ = proxy::run_server(proxy_cfg, proxy_ingest, proxy_running);
        });

        let tui_cfg = (*config).clone();
        tui::run_tui_app(&tui_cfg)?;

        running.store(false, Ordering::Relaxed);
        let _ = proxy_handle.join();
    } else {
        proxy::run_server(
            Arc::clone(&config),
            ingest_sender.clone(),
            Arc::clone(&running),
        )?;
    }

    info!("Proxy listener stopped. Flushing remaining telemetry to SQLite...");
    drop(ingest_sender);
    let _ = ingest_handle.join();

    info!("ReqLens shutdown completed cleanly.");
    Ok(())
}
