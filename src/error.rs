use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReqLensError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Proxy upstream error: {0}")]
    Upstream(String),

    #[error("Storage engine error: {0}")]
    Storage(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ReqLensError>;
