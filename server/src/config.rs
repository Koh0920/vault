use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub listen_addr: String,
    pub frontend_dir: PathBuf,
    pub state_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub rclone_binary: PathBuf,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub google_redirect_uri: String,
    pub session_cookie_secret: Vec<u8>,
    pub job_poll_interval_ms: u64,
    pub default_crypt_remote_suffix: String,
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

pub fn load() -> AppConfig {
    let state_dir = env_var("VAULT_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("vault-data"));
    let temp_dir = env_var("VAULT_TEMP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| state_dir.join("tmp"));
    let frontend_dir = env_var("VAULT_FRONTEND_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../frontend/dist"));
    let rclone_binary = env_var("RCLONE_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("rclone"));
    let listen_addr = env_var("VAULT_LISTEN_ADDR").unwrap_or_else(|| "0.0.0.0:8080".to_string());
    let google_redirect_uri = env_var("GOOGLE_REDIRECT_URI")
        .unwrap_or_else(|| "http://localhost:8080/api/v1/drive/oauth/callback".to_string());
    let cookie_secret = env_var("VAULT_COOKIE_SECRET")
        .map(|s| s.as_bytes().to_vec())
        .unwrap_or_else(|| b"vault-dev-insecure-cookie-secret-change-me-0123456789".to_vec());

    AppConfig {
        listen_addr,
        frontend_dir,
        state_dir,
        temp_dir,
        rclone_binary,
        google_client_id: env_var("GOOGLE_CLIENT_ID"),
        google_client_secret: env_var("GOOGLE_CLIENT_SECRET"),
        google_redirect_uri,
        session_cookie_secret: cookie_secret,
        job_poll_interval_ms: 1000,
        default_crypt_remote_suffix: "-crypt".to_string(),
    }
}