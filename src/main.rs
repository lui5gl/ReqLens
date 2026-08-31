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
use reqlens::sniff::{self, SniffConfig};
use reqlens::tui::{self, TuiConfig, TuiSource};

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
            tui: tui_enabled,
        }) => {
            if !tui_enabled {
                tracing_subscriber::registry()
                    .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
                    .with(tracing_subscriber::fmt::layer())
                    .init();
            }

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
            if tui_enabled {
                let sniff_ingest = ingest_sender.clone();
                let sniff_running = Arc::clone(&running);
                let sniff_handle =
                    thread::spawn(move || sniff::run_sniffer(config, sniff_ingest, sniff_running));

                // Surface immediate startup failures (permissions/interface)
                // before taking ownership of the terminal.
                thread::sleep(std::time::Duration::from_millis(50));
                if sniff_handle.is_finished() {
                    let result = sniff_handle
                        .join()
                        .map_err(|_| "passive capture thread panicked")?;
                    running.store(false, Ordering::Relaxed);
                    drop(ingest_sender);
                    let _ = ingest_handle.join();
                    result?;
                    return Err("passive capture stopped during startup".into());
                }

                let tui_config = TuiConfig {
                    db_path,
                    source: TuiSource::Passive {
                        interface,
                        server_ip,
                        port,
                    },
                };
                let tui_result = tui::run_tui_app(&tui_config);
                running.store(false, Ordering::Relaxed);
                let sniff_result = sniff_handle
                    .join()
                    .map_err(|_| "passive capture thread panicked")?;
                tui_result?;
                sniff_result?;
            } else {
                sniff::run_sniffer(config, ingest_sender.clone(), running)?;
            }
            drop(ingest_sender);
            let _ = ingest_handle.join();
            return Ok(());
        }
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
            let (upstream_addr, _) = parse_upstream(&upstream)?;
            let tui_cfg = TuiConfig {
                db_path,
                source: TuiSource::Proxy {
                    listen,
                    upstream: upstream_addr,
                },
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

    register_shutdown_signals(&running)?;

    if config.tui_enabled {
        let proxy_cfg = Arc::clone(&config);
        let proxy_ingest = ingest_sender.clone();
        let proxy_running = Arc::clone(&running);

        let proxy_handle = thread::spawn(move || {
            let _ = proxy::run_server(proxy_cfg, proxy_ingest, proxy_running);
        });

        let tui_cfg = TuiConfig {
            db_path: config.db_path.clone(),
            source: TuiSource::Proxy {
                listen: config.listen_addr.to_string(),
                upstream: config.upstream_addr.clone(),
            },
        };
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
