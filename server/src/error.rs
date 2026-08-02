use thiserror::Error;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("{0}")]
    Message(String),
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("rclone: {0}")]
    Rclone(String),
    #[error("rclone command failed: {0}")]
    RcloneCommand(#[from] crate::rclone::RcloneError),
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

    /// A safe, user-facing message for API responses. Internal error details
    /// (raw rclone stderr, which may embed paths or secrets) are generalized
    /// here; the full detail is available via `to_string()` for server logs.
    pub fn user_message(&self) -> String {
        match self {
            VaultError::Message(m) => m.clone(),
            VaultError::Crypto(m) => format!("crypto: {m}"),
            VaultError::Rclone(_) => {
                "a storage operation failed. Check the server logs for details.".to_string()
            }
            VaultError::RcloneCommand(e) => {
                if e.is_not_found() {
                    "not found".to_string()
                } else {
                    format!(
                        "storage operation failed ({}). Check the server logs for details.",
                        e.operation
                    )
                }
            }
            VaultError::Drive(m) => format!("google drive: {m}"),
            VaultError::Io(_) => "a storage I/O operation failed".to_string(),
            VaultError::Serde(_) => "invalid data received".to_string(),
            VaultError::Reqwest(_) => "upstream request failed".to_string(),
            VaultError::Sqlite(_) => "local database error".to_string(),
            VaultError::Meta(m) => format!("metadata: {m}"),
            VaultError::NotFound(m) => format!("not found: {m}"),
            VaultError::TooLarge(m) => format!("payload too large: {m}"),
            VaultError::Forbidden(m) => format!("forbidden: {m}"),
        }
    }
}

pub fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(VaultError::Message(message.into()))
}
