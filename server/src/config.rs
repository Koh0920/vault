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
    pub session_cookie_secure: bool,
    pub session_max_age_secs: u64,
    pub session_max_count: usize,
    pub job_poll_interval_ms: u64,
    pub default_crypt_remote_suffix: String,
    pub max_upload_bytes: u64,
    pub max_upload_files: usize,
    pub max_preview_bytes: usize,
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn env_parse<T>(name: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    env_var(name)
        .and_then(|v| v.parse::<T>().ok())
        .unwrap_or(default)
}

fn is_localhost(addr: &str) -> bool {
    let host = addr.split(':').next().unwrap_or("").trim();
    host == "localhost" || host == "127.0.0.1" || host == "[::1]" || host == "::1"
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

    let cookie_secure = env_parse("VAULT_COOKIE_SECURE", false);
    let session_max_age = env_parse("VAULT_SESSION_MAX_AGE_SECS", 60 * 60u64);
    let session_max_count = env_parse("VAULT_SESSION_MAX_COUNT", 256usize);

    // Refuse the well-known dev default secret on anything but loopback. A
    // remote deployment must provide a strong VAULT_COOKIE_SECRET.
    let cookie_secret = match env_var("VAULT_COOKIE_SECRET") {
        Some(secret) if secret.len() >= 32 => secret.into_bytes(),
        Some(_) => {
            if is_localhost(&listen_addr) {
                b"vault-dev-insecure-cookie-secret-change-me-0123456789".to_vec()
            } else {
                panic!("VAULT_COOKIE_SECRET must be set to a value of at least 32 bytes when not bound to loopback");
            }
        }
        None => {
            if is_localhost(&listen_addr) {
                b"vault-dev-insecure-cookie-secret-change-me-0123456789".to_vec()
            } else {
                panic!("VAULT_COOKIE_SECRET must be set when not bound to loopback");
            }
        }
    };

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
        session_cookie_secure: cookie_secure,
        session_max_age_secs: session_max_age,
        session_max_count,
        job_poll_interval_ms: 1000,
        default_crypt_remote_suffix: "-crypt".to_string(),
        max_upload_bytes: env_parse("VAULT_MAX_UPLOAD_BYTES", 4 * 1024 * 1024 * 1024u64),
        max_upload_files: env_parse("VAULT_MAX_UPLOAD_FILES", 200usize),
        max_preview_bytes: env_parse("VAULT_MAX_PREVIEW_BYTES", 5 * 1024 * 1024usize),
    }
}
