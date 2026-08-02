pub mod config;
pub mod crypto;
pub mod drive;
pub mod error;
pub mod manifest;
pub mod rclone;
pub mod session;
pub mod storage;
pub mod vault;

pub mod api;

pub use error::{Result, VaultError};

/// Generates an opaque session/auth id.
pub fn auth_key() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Clone)]
pub struct AppState {
    pub cfg: config::AppConfig,
    pub sessions: session::SessionStore,
}

impl AppState {
    pub fn new(cfg: config::AppConfig) -> Self {
        let sessions = session::SessionStore::new(&cfg);
        AppState { cfg, sessions }
    }
}
