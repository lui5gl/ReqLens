pub mod cli;

use self::cli::{AppConfig, CliArgs, Commands};
use crate::error::{ReqLensError, Result};
use clap::Parser;
use hyper::Uri;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

pub fn parse_cli() -> CliArgs {
    CliArgs::parse()
}

pub fn resolve_config(
    listen: &str,
    upstream: &str,
    db_path: PathBuf,
    max_body: usize,
    no_redact: bool,
    tui: bool,
) -> Result<AppConfig> {
    let listen_addr: SocketAddr = listen
        .parse()
        .map_err(|e| ReqLensError::Config(format!("Invalid listen address '{}': {}", listen, e)))?;

    let upstream_uri: Uri = upstream
        .parse()
        .map_err(|e| ReqLensError::Config(format!("Invalid upstream URI '{}': {}", upstream, e)))?;

    if upstream_uri.scheme().is_none() || upstream_uri.authority().is_none() {
        return Err(ReqLensError::Config(format!(
            "Upstream URI '{}' must include scheme (e.g. http://) and host:port",
            upstream
        )));
    }

    if max_body == 0 {
        return Err(ReqLensError::Config(
            "max_body must be greater than 0".into(),
        ));
    }

    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| {
            ReqLensError::Config(format!(
                "Failed to create database directory {:?}: {}",
                parent, e
            ))
        })?;
    }

    Ok(AppConfig {
        listen_addr,
        upstream_uri,
        db_path,
        max_body,
        redact_enabled: !no_redact,
        tui_enabled: tui,
    })
}

pub fn load_config() -> Result<AppConfig> {
    let args = parse_cli();
    match args.command {
        Some(Commands::Start {
            listen,
            upstream,
            db_path,
            max_body,
            no_redact,
            tui,
        }) => resolve_config(&listen, &upstream, db_path, max_body, no_redact, tui),
        Some(Commands::Tui {
            db_path,
            listen,
            upstream,
        }) => resolve_config(&listen, &upstream, db_path, DEFAULT_MAX_BODY, false, true),
        _ => resolve_config(
            &args.listen,
            &args.upstream,
            args.db_path,
            args.max_body,
            args.no_redact,
            args.tui,
        ),
    }
}
const DEFAULT_MAX_BODY: usize = 65536;
