use super::cli::CaptureMode;
use crate::error::{ReqLensError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

pub const INSTALLED_CONFIG_PATH: &str = "/etc/reqlens/config.json";
const INSTALLED_CONFIG_DIRECTORY: &str = "/etc/reqlens";
const INSTALLED_CONFIG_FILE: &str = "config.json";
const TEMPORARY_CONFIG_FILE: &str = "config.json.tmp";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InstalledConfig {
    pub mode: CaptureMode,
    pub interface: String,
    pub server_ip: Option<Ipv4Addr>,
    pub port: u16,
    pub listen: String,
    pub upstream: String,
    pub db_path: PathBuf,
    pub max_body: usize,
    pub no_redact: bool,
}

pub fn load_installed_config() -> Result<Option<InstalledConfig>> {
    let config_path = Path::new(INSTALLED_CONFIG_PATH);
    if !config_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(config_path)?;
    let config = serde_json::from_str(&content).map_err(|error| {
        ReqLensError::Config(format!(
            "invalid installed configuration '{}': {error}",
            config_path.display()
        ))
    })?;
    Ok(Some(config))
}

pub fn save_installed_config(config: &InstalledConfig) -> Result<()> {
    let directory = Path::new(INSTALLED_CONFIG_DIRECTORY);
    fs::create_dir_all(directory)?;
    let temporary_path = directory.join(TEMPORARY_CONFIG_FILE);
    let config_path = directory.join(INSTALLED_CONFIG_FILE);
    let content = serde_json::to_string_pretty(config)?;
    fs::write(&temporary_path, content)?;
    fs::rename(temporary_path, config_path)?;
    Ok(())
}
