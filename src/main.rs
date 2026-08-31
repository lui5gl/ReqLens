use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use reqlens::config::cli::{AppConfig, CaptureMode, Commands};
use reqlens::config::installed::load_installed_config;
use reqlens::config::{self, parse_cli};
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
            Ok(())
        }
        Some(Commands::Status { db_path }) => {
            let dashboard_config = dashboard_config(db_path)?;
            ops::print_status(&dashboard_config.db_path)?;
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
        Some(Commands::Tui { db_path }) => {
            tui::run_tui_app(&dashboard_config(db_path)?)?;
            Ok(())
        }
        Some(Commands::Start {
            listen,
            upstream,
            db_path,
            max_body,
            no_redact,
            tui,
        }) => {
            let config =
                config::resolve_config(&listen, &upstream, db_path, max_body, no_redact, tui)?;
            run_proxy(config)?;
            Ok(())
        }
        None => {
            tui::run_tui_app(&dashboard_config(None)?)?;
            Ok(())
        }
    }
}

fn run_proxy(config: AppConfig) -> Result<(), Box<dyn std::error::Error>> {
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

fn dashboard_config(
    db_path_override: Option<std::path::PathBuf>,
) -> Result<TuiConfig, Box<dyn std::error::Error>> {
    let installed = load_installed_config()?;
    let db_path = db_path_override
        .or_else(|| installed.as_ref().map(|config| config.db_path.clone()))
        .unwrap_or_else(|| std::path::PathBuf::from("/var/lib/reqlens/reqlens.db"));

    let source = match installed {
        Some(config) if config.mode == CaptureMode::Sniff => TuiSource::Passive {
            interface: config.interface,
            server_ip: config.server_ip,
            port: config.port,
        },
        Some(config) => TuiSource::Proxy {
            listen: config.listen,
            upstream: config.upstream,
        },
        None => TuiSource::Passive {
            interface: "unknown".into(),
            server_ip: None,
            port: 80,
        },
    };

    Ok(TuiConfig { db_path, source })
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
