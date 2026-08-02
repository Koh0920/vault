use thiserror::Error;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("{0}")]
    Message(String),
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("rclone: {0}")]
    Rclone(String),
    #[error("google drive: {0}")]
    Drive(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("metadata: {0}")]
    Meta(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("payload too large: {0}")]
    TooLarge(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
}

pub type Result<T> = std::result::Result<T, VaultError>;

impl VaultError {
    pub fn msg(message: impl Into<String>) -> Self {
        VaultError::Message(message.into())
    }
}

pub fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(VaultError::Message(message.into()))
}