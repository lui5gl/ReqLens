pub mod cli;
pub mod installed;

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
) -> Result<AppConfig> {
    let listen_addr: SocketAddr = listen
        .parse()
        .map_err(|e| ReqLensError::Config(format!("Invalid listen address '{}': {}", listen, e)))?;

    let (upstream_addr, upstream_host) = parse_upstream(upstream)?;

    validate_proxy_endpoints(listen_addr, &upstream_addr)?;

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
    })
}

/// Refuse the most common proxy self-loop: binding every local interface and
/// forwarding back to the same local port. Such a configuration recursively
/// proxies every request into ReqLens itself, creating connections until the
/// host CPU is exhausted. Apache must listen on a different port from ReqLens.
pub fn validate_proxy_endpoints(listen_addr: SocketAddr, upstream_addr: &str) -> Result<()> {
    let Ok(upstream) = upstream_addr.parse::<SocketAddr>() else {
        return Ok(());
    };

    let upstream_is_local = upstream.ip().is_loopback() || upstream.ip().is_unspecified();
    let listener_covers_localhost =
        listen_addr.ip().is_unspecified() || listen_addr.ip().is_loopback();

    if upstream.port() == listen_addr.port() && upstream_is_local && listener_covers_localhost {
        return Err(ReqLensError::Config(format!(
            "proxy loop detected: --listen {listen_addr} and --upstream http://{upstream_addr} use the same local port. ReqLens and Apache cannot both use that port. For example, configure Apache on 127.0.0.1:8080 and run ReqLens with --listen 0.0.0.0:80 --upstream http://127.0.0.1:8080"
        )));
    }

    Ok(())
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
        }) => resolve_config(&listen, &upstream, db_path, max_body, no_redact),
        _ => resolve_config(
            &args.listen,
            &args.upstream,
            args.db_path,
            args.max_body,
            args.no_redact,
        ),
    }
}
#[cfg(test)]
mod tests {
    use super::validate_proxy_endpoints;

    #[test]
    fn rejects_wildcard_listener_forwarding_to_same_local_port() {
        let error =
            validate_proxy_endpoints("0.0.0.0:80".parse().unwrap(), "127.0.0.1:80").unwrap_err();
        assert!(error.to_string().contains("proxy loop detected"));
    }

    #[test]
    fn accepts_separate_proxy_and_apache_ports() {
        validate_proxy_endpoints("0.0.0.0:80".parse().unwrap(), "127.0.0.1:8080").unwrap();
    }
}
