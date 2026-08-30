use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use reqlens::config::cli::Commands;
use reqlens::config::{self, parse_cli};
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
            let tui_cfg = reqlens::config::cli::AppConfig {
                listen_addr: listen
                    .parse()
                    .unwrap_or_else(|_| "0.0.0.0:8080".parse().unwrap()),
                upstream_uri: upstream
                    .parse()
                    .unwrap_or_else(|_| "http://127.0.0.1:80".parse().unwrap()),
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

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!(
                "\n❌ Error al inicializar el runtime asíncrono de red: {}",
                e
            );
            if e.raw_os_error() == Some(38) {
                eprintln!("\n⚠️  DETECCIÓN DE KERNEL LINUX LEGACY (Kernel < 2.6.32 / CentOS 5.x):");
                eprintln!(
                    "   El kernel de este sistema (2.6.18) no posee la llamada al sistema 'epoll_create1' (añadida en 2.6.27)."
                );
                eprintln!("   Para ejecutar en CentOS 5, compila un shim de compatibilidad con:");
                eprintln!("   gcc -Wall -O2 -fPIC -shared -o ./epoll_shim.so epoll_shim.c");
                eprintln!("   Y ejecuta con: LD_PRELOAD=./epoll_shim.so reqlens");
                eprintln!(
                    "   (Nota: En CentOS 6+, CentOS 7/8/9, Debian, Ubuntu, Alpine y RHEL 6+ funciona de forma 100% nativa).\n"
                );
            }
            std::process::exit(1);
        }
    };

    runtime.block_on(run_async_server(config))
}

async fn run_async_server(
    config: reqlens::config::cli::AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
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

    if config.tui_enabled {
        let (proxy_shutdown_tx, proxy_shutdown_rx) = oneshot::channel();
        let proxy_cfg = Arc::clone(&config);
        let proxy_ingest = ingest_sender.clone();

        let proxy_handle = tokio::spawn(async move {
            let _ = proxy::run_server(proxy_cfg, proxy_ingest, async move {
                let _ = proxy_shutdown_rx.await;
            })
            .await;
        });

        let tui_cfg = (*config).clone();
        let tui_handle = tokio::task::spawn_blocking(move || tui::run_tui_app(&tui_cfg));

        let _ = tui_handle.await?;
        let _ = proxy_shutdown_tx.send(());
        let _ = proxy_handle.await;
    } else {
        proxy::run_server(
            Arc::clone(&config),
            ingest_sender.clone(),
            shutdown_signal(),
        )
        .await?;
    }

    info!("Proxy listener stopped. Flushing remaining telemetry to SQLite...");
    drop(ingest_sender);

    match tokio::time::timeout(Duration::from_secs(5), ingest_handle).await {
        Ok(res) => {
            if let Err(e) = res {
                eprintln!("Ingest task terminated with error: {:?}", e);
            }
        }
        Err(_) => {
            eprintln!("Timeout waiting for database writer to flush.");
        }
    }

    info!("ReqLens shutdown completed cleanly.");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
