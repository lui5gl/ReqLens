use super::api::route_api;
use super::assets::asset;
use crate::error::{ReqLensError, Result};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::info;

const WEB_LISTEN_SECURITY_ERROR: &str = "web dashboard must bind to loopback; use 127.0.0.1 or ::1";

pub fn run_web_server(db_path: PathBuf, listen: SocketAddr) -> Result<()> {
    run_web_server_with_browser(db_path, listen, false)
}

pub fn run_web_server_and_open(db_path: PathBuf, listen: SocketAddr) -> Result<()> {
    run_web_server_with_browser(db_path, listen, true)
}

fn run_web_server_with_browser(
    db_path: PathBuf,
    listen: SocketAddr,
    open_dashboard: bool,
) -> Result<()> {
    if !listen.ip().is_loopback() {
        return Err(ReqLensError::Config(WEB_LISTEN_SECURITY_ERROR.into()));
    }

    let listener = TcpListener::bind(listen)?;
    info!("ReqLens web dashboard listening on http://{listen}");
    println!("ReqLens web dashboard: http://{listen}");
    if open_dashboard {
        let _ = open_browser(&format!("http://{listen}"));
    }

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => serve_connection(stream, &db_path)?,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub fn open_browser(url: &str) -> std::io::Result<()> {
    Command::new("xdg-open").arg(url).spawn().map(|_| ())
}

fn serve_connection(stream: TcpStream, database_path: &Path) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let request_target = request_line.split_whitespace().nth(1).unwrap_or("/");
    let path = request_target.split('?').next().unwrap_or("/");
    let mut stream = stream;
    if path.starts_with("/api/") {
        let response = route_api(request_target, database_path);
        write_response(
            &mut stream,
            response.status,
            "application/json",
            response.body.as_bytes(),
        )?;
    } else if let Some((body, content_type)) = asset(path) {
        write_response(&mut stream, 200, content_type, body)?;
    } else {
        write_response(&mut stream, 404, "text/plain", b"Not found")?;
    }
    stream.flush()?;
    Ok(())
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}
