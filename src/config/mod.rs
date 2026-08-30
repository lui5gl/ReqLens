pub mod cli;

use self::cli::{AppConfig, CliArgs, Commands};
use crate::error::{ReqLensError, Result};
use clap::Parser;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

pub fn parse_cli() -> CliArgs {
    CliArgs::parse()
}

pub fn parse_upstream(raw: &str) -> Result<(String, String)> {
    let clean = raw
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let host_port_part = clean.split('/').next().unwrap_or("");
    let parts: Vec<&str> = host_port_part.split(':').collect();
    let host = parts[0];
    if host.is_empty() {
        return Err(ReqLensError::Config(format!(
            "Invalid upstream address '{}'",
            raw
        )));
    }
    let port = if parts.len() > 1 {
        parts[1]
            .parse::<u16>()
            .map_err(|e| ReqLensError::Config(format!("Invalid upstream port: {}", e)))?
    } else if raw.starts_with("https://") {
        443
    } else {
        80
    };
    Ok((format!("{}:{}", host, port), host.to_string()))
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

    let (upstream_addr, upstream_host) = parse_upstream(upstream)?;

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
        upstream_addr,
        upstream_host,
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
