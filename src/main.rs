use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use reqlens::config::cli::{AppConfig, Commands, DEFAULT_DB_PATH};
use reqlens::config::installed::load_installed_config;
use reqlens::config::{self, parse_cli};
use reqlens::ingest;
use reqlens::ops;
use reqlens::proxy;
use reqlens::sniff::{self, SniffConfig};
use reqlens::web;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ops::auto_deploy_to_bin();

    let args = parse_cli();

    match args.command {
        Some(Commands::Sniff {
            interface,
            server_ip,
            port,
            db_path,
            max_body,
            no_redact,
        }) => {
            initialize_logging();

            if max_body == 0 {
                return Err("--max-body must be greater than zero".into());
            }
            if let Some(parent) = db_path.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent)?;
            }

            let (ingest_sender, ingest_handle) = ingest::start_ingest_worker(db_path.clone());
            let running = Arc::new(AtomicBool::new(true));
            register_shutdown_signals(&running)?;
            let config = SniffConfig {
                interface: interface.clone(),
                server_ip,
                port,
                max_body,
                redact_enabled: !no_redact,
            };
            sniff::run_sniffer(config, ingest_sender.clone(), running)?;
            drop(ingest_sender);
            let _ = ingest_handle.join();
            Ok(())
        }
        Some(Commands::Status { db_path }) => {
            ops::print_status(&dashboard_db_path(db_path)?)?;
            Ok(())
        }
        Some(Commands::Restart) => {
            ops::restart_service()?;
            Ok(())
        }
        Some(Commands::Disable) => {
            ops::disable_service()?;
            Ok(())
        }
        Some(Commands::Install {
            mode,
            interface,
            server_ip,
            port,
            listen,
            upstream,
            db_path,
            max_body,
            no_redact,
        }) => {
            ops::install_service(ops::InstallConfig {
                mode,
                interface: &interface,
                server_ip,
                port,
                listen: &listen,
                upstream: &upstream,
                db_path: &db_path,
                max_body,
                no_redact,
            })?;
            Ok(())
        }
        Some(Commands::Uninstall { purge }) => {
            ops::uninstall_service(purge)?;
            Ok(())
        }
        Some(Commands::Web { db_path, listen }) => {
            web::run_web_server_and_open(dashboard_db_path(db_path)?, listen)?;
            Ok(())
        }
        Some(Commands::Start {
            listen,
            upstream,
            db_path,
            max_body,
            no_redact,
        }) => {
            let config = config::resolve_config(&listen, &upstream, db_path, max_body, no_redact)?;
            run_proxy(config)?;
            Ok(())
        }
        None => {
            web::run_web_server_and_open(dashboard_db_path(None)?, "127.0.0.1:8420".parse()?)?;
            Ok(())
        }
    }
}

fn run_proxy(config: AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    initialize_logging();

    if !config.redact_enabled {
        warn!(
            "⚠️ SECRETS REDACTION IS DISABLED (--no-redact). Sensitive data will be written to disk!"
        );
    }

    let (ingest_sender, ingest_handle) = ingest::start_ingest_worker(config.db_path.clone());
    let config = Arc::new(config);
    let running = Arc::new(AtomicBool::new(true));

    register_shutdown_signals(&running)?;

    proxy::run_server(
        Arc::clone(&config),
        ingest_sender.clone(),
        Arc::clone(&running),
    )?;

    info!("Proxy listener stopped. Flushing remaining telemetry to SQLite...");
    drop(ingest_sender);
    let _ = ingest_handle.join();

    info!("ReqLens shutdown completed cleanly.");
    Ok(())
}

fn dashboard_db_path(
    db_path_override: Option<std::path::PathBuf>,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let installed = load_installed_config()?;
    let db_path = db_path_override
        .or_else(|| installed.as_ref().map(|config| config.db_path.clone()))
        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_DB_PATH));

    Ok(db_path)
}

fn initialize_logging() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();
}

fn register_shutdown_signals(running: &Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        let running = Arc::clone(running);
        // The handler only performs a lock-free atomic store, which is
        // async-signal-safe. `signal_hook::flag::register` sets a flag to true;
        // ReqLens uses the inverse `running` convention and therefore needs an
        // explicit false store.
        unsafe {
            signal_hook::low_level::register(signal, move || {
                running.store(false, Ordering::Relaxed);
            })?;
        }
    }
    Ok(())
}
