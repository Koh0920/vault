use base64::Engine;
use desky_guest_tauri::{builtin_result, serve_guest_http, CommandEnvelope, GuestContext};
use dotenvy::from_path_override;
use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};
use rustic_backend::BackendOptions;
use rustic_core::{
    BackupOptions, ConfigOptions, Credentials, IndexedFullStatus, IndexedIdsStatus, KeyOptions,
    LocalDestination, LsOptions, NoProgressBars, OpenStatus, PathList, Progress, ProgressBars,
    ProgressType, Repository, RepositoryBackends, RepositoryOptions, RestoreOptions,
    RusticProgress, SnapshotOptions,
};
use rustic_core::repofile::{SnapshotFile, SnapshotSummary};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tiny_http::{Response, Server};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use url::Url;

#[derive(Debug, Deserialize)]
struct PingPayload {
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct PingResponse {
    ok: bool,
    adapter: String,
    session_id: String,
    echo: String,
}

#[derive(Debug, Serialize)]
struct CheckEnvResponse {
    ok: bool,
    adapter: String,
    session_id: String,
    ato_guest_mode: Option<String>,
}

#[derive(Debug, Serialize)]
struct ProviderInfo {
    id: String,
    label: String,
    auth_kind: String,
}

#[derive(Debug, Serialize)]
struct ProvidersResponse {
    providers: Vec<ProviderInfo>,
}

#[derive(Debug, Serialize)]
struct ProviderStatusInfo {
    id: String,
    connected: bool,
}

#[derive(Debug, Serialize)]
struct ProviderStatusesResponse {
    providers: Vec<ProviderStatusInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectProviderRequest {
    provider: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectProviderResponse {
    ok: bool,
    provider: String,
    status: String,
    next_action: String,
    config_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeConfigResponse {
    config_path: String,
    state_dir: String,
    rc_addr: String,
    default_mode: String,
    use_keychain: bool,
    job_poll_interval_ms: u64,
    default_crypt_remote_suffix: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitVaultRepositoryRequest {
    provider: String,
    password: String,
    use_keychain: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InitVaultRepositoryResponse {
    repo_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartVaultBackupRequest {
    provider: String,
    source_paths: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartVaultBackupResponse {
    job_id: String,
    execute_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct VaultRepositoryInfo {
    repo_id: String,
    provider: String,
    backend_kind: String,
    repo_locator: String,
    display_name: String,
    created_at: String,
    last_snapshot_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct VaultSnapshotSummary {
    files_new: u64,
    files_changed: u64,
    files_unmodified: u64,
    dirs_new: u64,
    total_files_processed: u64,
    total_bytes_processed: u64,
    data_added: u64,
    data_added_packed: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct VaultSnapshotInfo {
    snapshot_id: String,
    repo_id: String,
    time: String,
    hostname: String,
    label: String,
    tags: Vec<String>,
    paths: Vec<String>,
    summary: Option<VaultSnapshotSummary>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct VaultEntry {
    name: String,
    display_name: String,
    path: String,
    is_dir: bool,
    size: u64,
    mod_time: Option<String>,
    mime_type: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListVaultRepositoriesResponse {
    repositories: Vec<VaultRepositoryInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListVaultSnapshotsRequest {
    repo_id: String,
    limit: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListVaultSnapshotsResponse {
    snapshots: Vec<VaultSnapshotInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListVaultEntriesRequest {
    repo_id: String,
    snapshot_id: String,
    path: Option<String>,
    query: Option<String>,
    offset: Option<u64>,
    limit: Option<u64>,
    refresh: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListVaultEntriesResponse {
    repository: VaultRepositoryInfo,
    snapshot: VaultSnapshotInfo,
    current_path: String,
    total_count: u64,
    next_offset: Option<u64>,
    entries: Vec<VaultEntry>,
    listed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VaultItemRequest {
    repo_id: String,
    snapshot_id: String,
    path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartJobResponse {
    job_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewExplorerItemResponse {
    name: String,
    path: String,
    mime_type: Option<String>,
    kind: String,
    text: Option<String>,
    image_data_url: Option<String>,
    truncated: bool,
    size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadExplorerItemResponse {
    saved_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupJobResult {
    snapshot_id: String,
    files_new: u64,
    files_changed: u64,
    files_unchanged: u64,
    dirs_new: u64,
    total_bytes_processed: u64,
    total_bytes_added: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetJobStatusRequest {
    job_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListJobsRequest {
    kind: Option<String>,
    status: Option<String>,
    limit: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListJobsResponse {
    jobs: Vec<JobStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct JobProgress {
    bytes_done: u64,
    bytes_total: Option<u64>,
    speed: Option<u64>,
    eta: Option<u64>,
    current_file: Option<String>,
    transfers: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct JobStatus {
    job_id: String,
    execute_id: String,
    kind: String,
    phase: String,
    progress: JobProgress,
    error: Option<String>,
    result: Option<Value>,
    started_at: Option<String>,
    finished_at: Option<String>,
}

#[derive(Debug, Clone)]
struct JobRecord {
    job_id: String,
    execute_id: String,
    kind: String,
    phase: String,
    provider: Option<String>,
    repo_id: Option<String>,
    source_ref: String,
    target_ref: String,
    display_name: String,
    progress: JobProgress,
    error: Option<String>,
    result: Option<Value>,
    started_at: Option<String>,
    finished_at: Option<String>,
}

#[derive(Debug, Clone)]
struct AppConfig {
    rclone_binary: String,
    config_path: PathBuf,
    state_dir: PathBuf,
    use_keychain: bool,
    job_poll_interval_ms: u64,
    default_gdrive_remote: String,
    google_drive_client_id: Option<String>,
    google_drive_client_secret: Option<String>,
    google_drive_callback_port: u16,
    google_drive_scopes: String,
    default_r2_remote: String,
    r2_bucket: Option<String>,
    r2_endpoint: Option<String>,
    r2_account_id: Option<String>,
    r2_access_key_id: Option<String>,
    r2_secret_access_key: Option<String>,
}

#[derive(Debug)]
struct GoogleDriveOAuthConfig {
    client_id: String,
    client_secret: String,
    callback_port: u16,
    scopes: String,
}

#[derive(Debug)]
struct R2Config {
    bucket: String,
    endpoint: String,
    access_key_id: String,
    secret_access_key: String,
}

#[derive(Debug)]
struct RusticJobSharedState {
    bytes_done: u64,
    bytes_total: Option<u64>,
    current_file: Option<String>,
    last_flush: Instant,
}

#[derive(Debug, Clone)]
struct RusticJobProgressBars {
    job_id: String,
    state: Arc<Mutex<RusticJobSharedState>>,
}

#[derive(Debug, Clone)]
struct RusticJobProgress {
    job_id: String,
    state: Arc<Mutex<RusticJobSharedState>>,
    progress_type: ProgressType,
}

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);
static JOBS: OnceLock<Mutex<HashMap<String, JobRecord>>> = OnceLock::new();
static APP_CONFIG: OnceLock<AppConfig> = OnceLock::new();
static DB_STARTUP_STATE: OnceLock<()> = OnceLock::new();
static DOTENV_STATE: OnceLock<()> = OnceLock::new();
static VAULT_PASSWORDS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn sample_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("sample root")
        .to_path_buf()
}

fn ensure_dotenv_loaded() {
    DOTENV_STATE.get_or_init(|| {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let current_dir = std::env::current_dir().ok();
        let candidate_paths = [
            Some(sample_root().join(".env")),
            Some(manifest_dir.join(".env")),
            current_dir.as_ref().map(|path| path.join(".env")),
            current_dir
                .as_ref()
                .and_then(|path| path.parent().map(|parent| parent.join(".env"))),
        ];

        for path in candidate_paths.into_iter().flatten() {
            if path.exists() {
                let _ = from_path_override(path);
                break;
            }
        }
    });
}

fn guest_context() -> GuestContext {
    GuestContext::from_env("tauri", 43150, sample_root())
}

fn jobs() -> &'static Mutex<HashMap<String, JobRecord>> {
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn vault_passwords() -> &'static Mutex<HashMap<String, String>> {
    VAULT_PASSWORDS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn upload_timestamp() -> Result<String, String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| error.to_string())
}

fn app_config() -> &'static AppConfig {
    APP_CONFIG.get_or_init(|| {
        ensure_dotenv_loaded();

        let state_dir = std::env::var("APP_STATE_DIR")
            .ok()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| sample_root().join(".state"));
        let config_path = std::env::var("APP_RCLONE_CONFIG_PATH")
            .ok()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| state_dir.join("rclone").join("rclone.conf"));

        AppConfig {
            rclone_binary: std::env::var("RCLONE_SIDECAR_NAME")
                .unwrap_or_else(|_| "rclone".to_string()),
            config_path,
            state_dir,
            use_keychain: env_flag("APP_USE_KEYCHAIN", true),
            job_poll_interval_ms: std::env::var("VITE_JOB_POLL_INTERVAL_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(1000),
            default_gdrive_remote: std::env::var("DEFAULT_GDRIVE_REMOTE")
                .unwrap_or_else(|_| "drive".to_string()),
            google_drive_client_id: std::env::var("GOOGLE_DRIVE_CLIENT_ID").ok(),
            google_drive_client_secret: std::env::var("GOOGLE_DRIVE_CLIENT_SECRET").ok(),
            google_drive_callback_port: std::env::var("GOOGLE_DRIVE_CALLBACK_PORT")
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(53682),
            google_drive_scopes: std::env::var("GOOGLE_DRIVE_SCOPES")
                .unwrap_or_else(|_| "https://www.googleapis.com/auth/drive".to_string()),
            default_r2_remote: std::env::var("DEFAULT_R2_REMOTE")
                .unwrap_or_else(|_| "r2".to_string()),
            r2_bucket: std::env::var("R2_BUCKET").ok().filter(|value| !value.is_empty()),
            r2_endpoint: std::env::var("R2_ENDPOINT").ok().filter(|value| !value.is_empty()),
            r2_account_id: std::env::var("R2_ACCOUNT_ID").ok().filter(|value| !value.is_empty()),
            r2_access_key_id: std::env::var("R2_ACCESS_KEY_ID").ok().filter(|value| !value.is_empty()),
            r2_secret_access_key: std::env::var("R2_SECRET_ACCESS_KEY")
                .ok()
                .filter(|value| !value.is_empty()),
        }
    })
}

fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(default)
}

fn google_drive_oauth_config() -> Result<GoogleDriveOAuthConfig, String> {
    let config = app_config();
    Ok(GoogleDriveOAuthConfig {
        client_id: config
            .google_drive_client_id
            .clone()
            .ok_or_else(|| "GOOGLE_DRIVE_CLIENT_ID is not set".to_string())?,
        client_secret: config
            .google_drive_client_secret
            .clone()
            .ok_or_else(|| "GOOGLE_DRIVE_CLIENT_SECRET is not set".to_string())?,
        callback_port: config.google_drive_callback_port,
        scopes: config.google_drive_scopes.clone(),
    })
}

fn resolve_r2_config() -> Result<R2Config, String> {
    let config = app_config();
    let bucket = config
        .r2_bucket
        .clone()
        .ok_or_else(|| "R2_BUCKET is not set".to_string())?;
    let access_key_id = config
        .r2_access_key_id
        .clone()
        .ok_or_else(|| "R2_ACCESS_KEY_ID is not set".to_string())?;
    let secret_access_key = config
        .r2_secret_access_key
        .clone()
        .ok_or_else(|| "R2_SECRET_ACCESS_KEY is not set".to_string())?;
    let endpoint = config.r2_endpoint.clone().or_else(|| {
        config
            .r2_account_id
            .as_ref()
            .map(|account_id| format!("https://{account_id}.r2.cloudflarestorage.com"))
    });

    Ok(R2Config {
        bucket,
        endpoint: endpoint.ok_or_else(|| "R2_ENDPOINT or R2_ACCOUNT_ID is not set".to_string())?,
        access_key_id,
        secret_access_key,
    })
}

fn ensure_state_paths() -> Result<(), String> {
    let config = app_config();
    fs::create_dir_all(&config.state_dir).map_err(|error| error.to_string())?;
    if let Some(parent) = config.config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if !config.config_path.exists() {
        fs::write(&config.config_path, b"").map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn app_db_path() -> PathBuf {
    app_config().state_dir.join("app.db")
}

fn open_app_db() -> Result<Connection, String> {
    ensure_state_paths()?;
    let db_path = app_db_path();
    let connection =
        Connection::open(&db_path).map_err(|error| format!("Failed to open app db ({db_path:?}): {error}"))?;
    init_app_db(&connection)?;
    DB_STARTUP_STATE.get_or_init(|| {
        let _ = mark_stale_jobs_failed(&connection);
    });
    Ok(connection)
}

fn init_app_db(connection: &Connection) -> Result<(), String> {
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap_or(0);

    if version < 2 {
        connection
            .execute_batch(
                "
                DROP TABLE IF EXISTS upload_index;
                DROP TABLE IF EXISTS explorer_entries;
                DROP TABLE IF EXISTS transfer_jobs;
                DROP TABLE IF EXISTS vault_repositories;
                DROP TABLE IF EXISTS snapshots;
                DROP TABLE IF EXISTS snapshot_entries_cache;
                ",
            )
            .map_err(|error| error.to_string())?;
    }

    connection
        .execute_batch(
            "
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS vault_repositories (
                repo_id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                backend_kind TEXT NOT NULL,
                repo_locator TEXT NOT NULL,
                display_name TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_snapshot_at TEXT
            );
            CREATE TABLE IF NOT EXISTS snapshots (
                snapshot_id TEXT NOT NULL,
                repo_id TEXT NOT NULL,
                time TEXT NOT NULL,
                hostname TEXT NOT NULL,
                label TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                paths_json TEXT NOT NULL,
                summary_json TEXT,
                PRIMARY KEY (snapshot_id, repo_id)
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_repo_time
                ON snapshots (repo_id, time DESC);
            CREATE TABLE IF NOT EXISTS snapshot_entries_cache (
                repo_id TEXT NOT NULL,
                snapshot_id TEXT NOT NULL,
                dir_path TEXT NOT NULL,
                entry_path TEXT NOT NULL,
                name TEXT NOT NULL,
                is_dir INTEGER NOT NULL,
                size INTEGER NOT NULL,
                mod_time TEXT,
                mime_type TEXT,
                listed_at TEXT NOT NULL,
                PRIMARY KEY (repo_id, snapshot_id, dir_path, entry_path)
            );
            CREATE INDEX IF NOT EXISTS idx_snapshot_entries_dir
                ON snapshot_entries_cache (repo_id, snapshot_id, dir_path);
            CREATE INDEX IF NOT EXISTS idx_snapshot_entries_dir_name
                ON snapshot_entries_cache (repo_id, snapshot_id, dir_path, name);
            CREATE TABLE IF NOT EXISTS transfer_jobs (
                job_id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                execute_id TEXT NOT NULL,
                provider TEXT,
                repo_id TEXT,
                source_ref TEXT NOT NULL,
                target_ref TEXT NOT NULL,
                display_name TEXT NOT NULL,
                bytes_done INTEGER NOT NULL,
                bytes_total INTEGER,
                speed INTEGER,
                eta INTEGER,
                current_item TEXT,
                error TEXT,
                started_at TEXT,
                finished_at TEXT,
                result_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_transfer_jobs_kind_status
                ON transfer_jobs (kind, status, started_at DESC);
            CREATE INDEX IF NOT EXISTS idx_transfer_jobs_started_at
                ON transfer_jobs (started_at DESC);
            ",
        )
        .map_err(|error| error.to_string())?;

    connection
        .pragma_update(None, "user_version", 2_i64)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn mark_stale_jobs_failed(connection: &Connection) -> Result<(), String> {
    connection
        .execute(
            "
            UPDATE transfer_jobs
            SET status = 'failed',
                error = COALESCE(error, 'App restarted before job completion'),
                finished_at = COALESCE(finished_at, ?1),
                speed = NULL,
                eta = NULL
            WHERE status = 'running'
            ",
            [upload_timestamp()?],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn runtime_config() -> RuntimeConfigResponse {
    let config = app_config();
    RuntimeConfigResponse {
        config_path: config.config_path.display().to_string(),
        state_dir: config.state_dir.display().to_string(),
        rc_addr: String::new(),
        default_mode: "vault".to_string(),
        use_keychain: config.use_keychain,
        job_poll_interval_ms: config.job_poll_interval_ms,
        default_crypt_remote_suffix: String::new(),
    }
}

fn rc_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .expect("reqwest client")
}

fn write_rclone_section(section_name: &str, entries: &[(&str, String)]) -> Result<(), String> {
    ensure_state_paths()?;
    let config_path = &app_config().config_path;
    let existing = if config_path.exists() {
        fs::read_to_string(config_path).map_err(|error| error.to_string())?
    } else {
        String::new()
    };

    let mut lines = Vec::new();
    let mut skipping = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let current = &trimmed[1..trimmed.len() - 1];
            skipping = current == section_name;
            if skipping {
                continue;
            }
        }
        if !skipping {
            lines.push(line.to_string());
        }
    }

    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    if !lines.is_empty() {
        lines.push(String::new());
    }
    lines.push(format!("[{section_name}]"));
    for (key, value) in entries {
        lines.push(format!("{key} = {value}"));
    }
    lines.push(String::new());

    fs::write(config_path, lines.join("\n")).map_err(|error| error.to_string())
}

fn read_rclone_section(section_name: &str) -> Result<HashMap<String, String>, String> {
    ensure_state_paths()?;
    let config_path = &app_config().config_path;
    let existing = fs::read_to_string(config_path).map_err(|error| error.to_string())?;

    let mut in_section = false;
    let mut entries = HashMap::new();
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let current = &trimmed[1..trimmed.len() - 1];
            if in_section && current != section_name {
                break;
            }
            in_section = current == section_name;
            continue;
        }
        if !in_section || trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            entries.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    Ok(entries)
}

fn update_rclone_section_value(section_name: &str, target_key: &str, target_value: &str) -> Result<(), String> {
    ensure_state_paths()?;
    let config_path = &app_config().config_path;
    let existing = fs::read_to_string(config_path).map_err(|error| error.to_string())?;
    let mut lines = Vec::new();
    let mut in_section = false;
    let mut updated = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_section && !updated {
                lines.push(format!("{target_key} = {target_value}"));
                updated = true;
            }
            let current = &trimmed[1..trimmed.len() - 1];
            in_section = current == section_name;
            lines.push(line.to_string());
            continue;
        }

        if in_section {
            if let Some((key, _)) = trimmed.split_once('=') {
                if key.trim() == target_key {
                    lines.push(format!("{target_key} = {target_value}"));
                    updated = true;
                    continue;
                }
            }
        }

        lines.push(line.to_string());
    }

    if in_section && !updated {
        lines.push(format!("{target_key} = {target_value}"));
    }

    fs::write(config_path, lines.join("\n")).map_err(|error| error.to_string())
}

fn has_rclone_section(section_name: &str) -> Result<bool, String> {
    ensure_state_paths()?;
    let config_path = &app_config().config_path;
    let existing = fs::read_to_string(config_path).map_err(|error| error.to_string())?;
    Ok(existing.lines().any(|line| line.trim() == format!("[{section_name}]")))
}

fn open_url_in_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    };

    command.spawn().map_err(|error| error.to_string())?;
    Ok(())
}

fn capture_google_drive_code(callback_port: u16, expected_state: &str) -> Result<String, String> {
    let server = Server::http(("127.0.0.1", callback_port)).map_err(|error| error.to_string())?;

    for _ in 0..10 {
        let request = server
            .recv_timeout(Duration::from_secs(180))
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Timed out waiting for Google OAuth callback".to_string())?;

        let callback_url = format!("http://127.0.0.1:{callback_port}{}", request.url());
        let parsed = Url::parse(&callback_url).map_err(|error| error.to_string())?;
        let query: HashMap<_, _> = parsed.query_pairs().into_owned().collect();

        if !query.contains_key("code") {
            let _ = request.respond(Response::empty(404));
            continue;
        }

        let _ = request.respond(Response::from_string(
            "Google Drive authorization received. You can return to the app.",
        ));

        if query.get("state").map(String::as_str) != Some(expected_state) {
            return Err("OAuth state mismatch".to_string());
        }

        return query
            .get("code")
            .cloned()
            .ok_or_else(|| "OAuth callback did not include code".to_string());
    }

    Err("Too many invalid requests received on callback port".to_string())
}

fn exchange_google_drive_code(
    config: &GoogleDriveOAuthConfig,
    code: &str,
) -> Result<Value, String> {
    let redirect_uri = format!("http://127.0.0.1:{}/auth/callback", config.callback_port);
    let response = rc_client()
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .map_err(|error| format!("Network request failed: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("google token exchange failed {status}: {body}"));
    }

    response.json::<Value>().map_err(|error| error.to_string())
}

fn parse_token_expiry(token: &Value) -> Result<Option<String>, String> {
    if let Some(expiry) = token.get("expiry").and_then(Value::as_str) {
        return Ok(Some(expiry.to_string()));
    }

    let expires_in = token.get("expires_in").and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|n| i64::try_from(n).ok()))
            .or_else(|| value.as_str().and_then(|s| s.parse::<i64>().ok()))
    });

    Ok(expires_in.map(|seconds| {
        (OffsetDateTime::now_utc() + time::Duration::seconds(seconds.max(0)))
            .format(&Rfc3339)
            .unwrap_or_else(|_| OffsetDateTime::now_utc().format(&Rfc3339).unwrap())
    }))
}

fn normalize_google_drive_token(token: &Value, force_refresh: bool) -> Result<String, String> {
    let access_token = token
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "Google token missing access_token".to_string())?;
    let token_type = token
        .get("token_type")
        .and_then(Value::as_str)
        .unwrap_or("Bearer");
    let refresh_token = token
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());

    let expiry = if force_refresh {
        (OffsetDateTime::now_utc() - time::Duration::seconds(60))
            .format(&Rfc3339)
            .map_err(|error| error.to_string())?
    } else {
        parse_token_expiry(token)?
            .unwrap_or_else(|| OffsetDateTime::now_utc().format(&Rfc3339).unwrap())
    };

    let mut normalized = serde_json::Map::new();
    normalized.insert("access_token".to_string(), json!(access_token));
    normalized.insert("token_type".to_string(), json!(token_type));
    normalized.insert("expiry".to_string(), json!(expiry));
    if let Some(refresh_token) = refresh_token {
        normalized.insert("refresh_token".to_string(), json!(refresh_token));
    }

    serde_json::to_string(&Value::Object(normalized)).map_err(|error| error.to_string())
}

fn ensure_drive_remote_token_compatibility(remote_name: &str) -> Result<(), String> {
    let section = read_rclone_section(remote_name)?;
    let token_value = match section.get("token") {
        Some(value) => value,
        None => return Ok(()),
    };
    let token = serde_json::from_str::<Value>(token_value).map_err(|error| error.to_string())?;
    if token.get("expiry").is_some() {
        return Ok(());
    }
    let normalized = normalize_google_drive_token(&token, true)?;
    update_rclone_section_value(remote_name, "token", &normalized)
}

fn connect_google_drive(config_path: String) -> ConnectProviderResponse {
    let oauth = match google_drive_oauth_config() {
        Ok(config) => config,
        Err(error) => {
            return ConnectProviderResponse {
                ok: false,
                provider: "drive".to_string(),
                status: "missing_env".to_string(),
                next_action: error,
                config_path,
            };
        }
    };

    let redirect_uri = format!("http://127.0.0.1:{}/auth/callback", oauth.callback_port);
    let state = format!("drive-auth-{}", JOB_COUNTER.fetch_add(1, Ordering::Relaxed));
    let auth_url = match Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", oauth.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", oauth.scopes.as_str()),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", state.as_str()),
        ],
    ) {
        Ok(url) => url,
        Err(error) => {
            return ConnectProviderResponse {
                ok: false,
                provider: "drive".to_string(),
                status: "failed".to_string(),
                next_action: error.to_string(),
                config_path,
            };
        }
    };

    if let Err(error) = open_url_in_browser(auth_url.as_str()) {
        return ConnectProviderResponse {
            ok: false,
            provider: "drive".to_string(),
            status: "failed".to_string(),
            next_action: error,
            config_path,
        };
    }

    let code = match capture_google_drive_code(oauth.callback_port, &state) {
        Ok(code) => code,
        Err(error) => {
            return ConnectProviderResponse {
                ok: false,
                provider: "drive".to_string(),
                status: "failed".to_string(),
                next_action: error,
                config_path,
            };
        }
    };

    let token = match exchange_google_drive_code(&oauth, &code) {
        Ok(token) => token,
        Err(error) => {
            return ConnectProviderResponse {
                ok: false,
                provider: "drive".to_string(),
                status: "failed".to_string(),
                next_action: error,
                config_path,
            };
        }
    };

    let token_json = match normalize_google_drive_token(&token, false) {
        Ok(value) => value,
        Err(error) => {
            return ConnectProviderResponse {
                ok: false,
                provider: "drive".to_string(),
                status: "failed".to_string(),
                next_action: error,
                config_path,
            };
        }
    };

    let remote_name = app_config().default_gdrive_remote.clone();
    let entries = [
        ("type", "drive".to_string()),
        ("scope", "drive".to_string()),
        ("token", token_json),
        ("client_id", oauth.client_id.clone()),
        ("client_secret", oauth.client_secret.clone()),
    ];

    match write_rclone_section(&remote_name, &entries) {
        Ok(_) => ConnectProviderResponse {
            ok: true,
            provider: "drive".to_string(),
            status: "configured".to_string(),
            next_action: format!("Remote `{remote_name}` configured in {config_path}"),
            config_path,
        },
        Err(error) => ConnectProviderResponse {
            ok: false,
            provider: "drive".to_string(),
            status: "failed".to_string(),
            next_action: error,
            config_path,
        },
    }
}

fn resolve_rclone_binary_path(binary_name: &str) -> PathBuf {
    let requested = PathBuf::from(binary_name);
    if requested.is_absolute() || requested.components().count() > 1 {
        return requested;
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bundled = exe_dir.join(&requested);
            if bundled.is_file() {
                return bundled;
            }
        }
    }
    requested
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn rclone_command_for_rustic() -> String {
    let config = app_config();
    let binary = resolve_rclone_binary_path(&config.rclone_binary);
    format!(
        "{} serve restic --addr localhost:0 --config {}",
        shell_quote(&binary.display().to_string()),
        shell_quote(&config.config_path.display().to_string())
    )
}

fn build_repository_backends(provider: &str) -> Result<RepositoryBackends, String> {
    let mut options = BTreeMap::new();
    let repository = match provider {
        "drive" => {
            ensure_drive_remote_token_compatibility(&app_config().default_gdrive_remote)?;
            options.insert("rclone-command".to_string(), rclone_command_for_rustic());
            format!("rclone:{}:.vault", app_config().default_gdrive_remote)
        }
        "r2" => {
            let r2 = resolve_r2_config()?;
            options.insert("bucket".to_string(), r2.bucket);
            options.insert("endpoint".to_string(), r2.endpoint);
            options.insert("access_key_id".to_string(), r2.access_key_id);
            options.insert("secret_access_key".to_string(), r2.secret_access_key);
            options.insert("region".to_string(), "auto".to_string());
            options.insert("root".to_string(), ".vault".to_string());
            "opendal:s3".to_string()
        }
        _ => return Err(format!("Unsupported provider: {provider}")),
    };

    let mut backend_options = BackendOptions::default();
    backend_options.repository = Some(repository);
    backend_options.options = options;
    backend_options
        .to_backends()
        .map_err(|error| error.to_string())
}

fn open_or_init_repository<P: ProgressBars + Clone>(
    provider: &str,
    password: &str,
    progress: P,
) -> Result<Repository<OpenStatus>, String> {
    let credentials = Credentials::password(password);
    let repo_opts = RepositoryOptions::default();
    let backends = build_repository_backends(provider)?;
    let repo = Repository::new_with_progress(&repo_opts, &backends, progress.clone())
        .map_err(|error| error.to_string())?;

    match repo.open(&credentials) {
        Ok(repo) => Ok(repo),
        Err(open_error) => {
            let backends = build_repository_backends(provider)?;
            let repo = Repository::new_with_progress(&repo_opts, &backends, progress)
                .map_err(|error| error.to_string())?;
            match repo.init(&credentials, &KeyOptions::default(), &ConfigOptions::default()) {
                Ok(repo) => Ok(repo),
                Err(init_error) => {
                    let init_message = init_error.to_string();
                    if init_message.contains("Config file already exists") {
                        Err(open_error.to_string())
                    } else {
                        Err(init_message)
                    }
                }
            }
        }
    }
}

fn open_indexed_ids_repository<P: ProgressBars + Clone>(
    provider: &str,
    password: &str,
    progress: P,
) -> Result<Repository<IndexedIdsStatus>, String> {
    open_or_init_repository(provider, password, progress)?
        .to_indexed_ids()
        .map_err(|error| error.to_string())
}

fn open_indexed_full_repository(
    provider: &str,
    password: &str,
) -> Result<Repository<IndexedFullStatus>, String> {
    let credentials = Credentials::password(password);
    let repo_opts = RepositoryOptions::default();
    let backends = build_repository_backends(provider)?;
    Repository::new_with_progress(&repo_opts, &backends, NoProgressBars)
        .map_err(|error| error.to_string())?
        .open(&credentials)
        .map_err(|error| error.to_string())?
        .to_indexed()
        .map_err(|error| error.to_string())
}

fn repo_id_for_provider(provider: &str) -> Result<String, String> {
    match provider {
        "drive" | "r2" => Ok(provider.to_string()),
        _ => Err(format!("Unsupported provider: {provider}")),
    }
}

fn repo_locator_for_provider(provider: &str) -> Result<String, String> {
    match provider {
        "drive" => Ok(format!("rclone:{}:.vault", app_config().default_gdrive_remote)),
        "r2" => Ok("opendal:s3".to_string()),
        _ => Err(format!("Unsupported provider: {provider}")),
    }
}

fn repo_backend_kind(provider: &str) -> Result<String, String> {
    match provider {
        "drive" => Ok("drive-via-rclone".to_string()),
        "r2" => Ok("r2-via-opendal".to_string()),
        _ => Err(format!("Unsupported provider: {provider}")),
    }
}

fn repo_display_name(provider: &str) -> Result<String, String> {
    match provider {
        "drive" => Ok("Google Drive Vault".to_string()),
        "r2" => Ok("Cloudflare R2 Vault".to_string()),
        _ => Err(format!("Unsupported provider: {provider}")),
    }
}

fn keyring_key(repo_id: &str) -> String {
    format!("vault:{repo_id}")
}

fn store_vault_password(repo_id: &str, password: &str, use_keychain: bool) -> Result<(), String> {
    if use_keychain && app_config().use_keychain {
        let entry = keyring::Entry::new("byok-encrypted-r2-drop", &keyring_key(repo_id))
            .map_err(|error| error.to_string())?;
        entry.set_password(password).map_err(|error| error.to_string())?;
    }
    vault_passwords()
        .lock()
        .map_err(|_| "password registry unavailable".to_string())?
        .insert(repo_id.to_string(), password.to_string());
    Ok(())
}

fn load_vault_password(repo_id: &str) -> Result<Option<String>, String> {
    if let Some(password) = vault_passwords()
        .lock()
        .map_err(|_| "password registry unavailable".to_string())?
        .get(repo_id)
        .cloned()
    {
        return Ok(Some(password));
    }

    if app_config().use_keychain {
        let entry = keyring::Entry::new("byok-encrypted-r2-drop", &keyring_key(repo_id))
            .map_err(|error| error.to_string())?;
        match entry.get_password() {
            Ok(value) => return Ok(Some(value)),
            Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(error.to_string()),
        }
    }

    Ok(std::env::var("APP_RUSTIC_PASSWORD").ok())
}

fn require_vault_password(repo_id: &str) -> Result<String, String> {
    load_vault_password(repo_id)?
        .ok_or_else(|| format!("No repository password is available for `{repo_id}`"))
}

fn sanitize_relative_path(path: Option<&str>) -> Result<String, String> {
    let raw = path.unwrap_or("").trim().trim_matches('/');
    if raw.is_empty() {
        return Ok(String::new());
    }
    let mut components = Vec::new();
    for component in raw.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            return Err("Invalid path".to_string());
        }
        components.push(component);
    }
    Ok(components.join("/"))
}

fn join_relative_path(base: &str, relative: &str) -> String {
    let base = base.trim_matches('/');
    let relative = relative.trim_matches('/');
    match (base.is_empty(), relative.is_empty()) {
        (true, true) => String::new(),
        (false, true) => base.to_string(),
        (true, false) => relative.to_string(),
        (false, false) => format!("{base}/{relative}"),
    }
}

fn path_basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(OsStr::to_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| path.to_string())
}

fn string_list_to_vec(value: impl ToString) -> Vec<String> {
    let rendered = value.to_string();
    if rendered.trim().is_empty() {
        Vec::new()
    } else {
        rendered
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(ToString::to_string)
            .collect()
    }
}

fn snapshot_summary_from_rustic(summary: &SnapshotSummary) -> VaultSnapshotSummary {
    VaultSnapshotSummary {
        files_new: summary.files_new,
        files_changed: summary.files_changed,
        files_unmodified: summary.files_unmodified,
        dirs_new: summary.dirs_new,
        total_files_processed: summary.total_files_processed,
        total_bytes_processed: summary.total_bytes_processed,
        data_added: summary.data_added,
        data_added_packed: summary.data_added_packed,
    }
}

fn snapshot_info_from_snapshot(repo_id: &str, snapshot: &SnapshotFile) -> VaultSnapshotInfo {
    VaultSnapshotInfo {
        snapshot_id: snapshot.id.to_string(),
        repo_id: repo_id.to_string(),
        time: snapshot.time.to_string(),
        hostname: snapshot.hostname.clone(),
        label: snapshot.label.clone(),
        tags: string_list_to_vec(snapshot.tags.clone()),
        paths: string_list_to_vec(snapshot.paths.clone()),
        summary: snapshot.summary.as_ref().map(snapshot_summary_from_rustic),
    }
}

fn upsert_vault_repository(provider: &str, snapshot_time: Option<&str>) -> Result<VaultRepositoryInfo, String> {
    let repo_id = repo_id_for_provider(provider)?;
    let backend_kind = repo_backend_kind(provider)?;
    let repo_locator = repo_locator_for_provider(provider)?;
    let display_name = repo_display_name(provider)?;
    let created_at = upload_timestamp()?;
    let connection = open_app_db()?;
    connection
        .execute(
            "
            INSERT INTO vault_repositories (repo_id, provider, backend_kind, repo_locator, display_name, created_at, last_snapshot_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(repo_id) DO UPDATE SET
                provider = excluded.provider,
                backend_kind = excluded.backend_kind,
                repo_locator = excluded.repo_locator,
                display_name = excluded.display_name,
                last_snapshot_at = COALESCE(excluded.last_snapshot_at, vault_repositories.last_snapshot_at)
            ",
            params![
                repo_id,
                provider,
                backend_kind,
                repo_locator,
                display_name,
                created_at,
                snapshot_time
            ],
        )
        .map_err(|error| error.to_string())?;
    read_vault_repository(&repo_id)
}

fn read_vault_repository(repo_id: &str) -> Result<VaultRepositoryInfo, String> {
    let connection = open_app_db()?;
    connection
        .query_row(
            "
            SELECT repo_id, provider, backend_kind, repo_locator, display_name, created_at, last_snapshot_at
            FROM vault_repositories
            WHERE repo_id = ?1
            ",
            [repo_id],
            |row| {
                Ok(VaultRepositoryInfo {
                    repo_id: row.get("repo_id")?,
                    provider: row.get("provider")?,
                    backend_kind: row.get("backend_kind")?,
                    repo_locator: row.get("repo_locator")?,
                    display_name: row.get("display_name")?,
                    created_at: row.get("created_at")?,
                    last_snapshot_at: row.get("last_snapshot_at")?,
                })
            },
        )
        .map_err(|error| format!("unknown repository `{repo_id}`: {error}"))
}

fn list_vault_repository_rows() -> Result<Vec<VaultRepositoryInfo>, String> {
    let connection = open_app_db()?;
    let mut statement = connection
        .prepare(
            "
            SELECT repo_id, provider, backend_kind, repo_locator, display_name, created_at, last_snapshot_at
            FROM vault_repositories
            ORDER BY created_at DESC
            ",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(VaultRepositoryInfo {
                repo_id: row.get("repo_id")?,
                provider: row.get("provider")?,
                backend_kind: row.get("backend_kind")?,
                repo_locator: row.get("repo_locator")?,
                display_name: row.get("display_name")?,
                created_at: row.get("created_at")?,
                last_snapshot_at: row.get("last_snapshot_at")?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
}

fn replace_snapshots_cache(repo_id: &str, snapshots: &[VaultSnapshotInfo]) -> Result<(), String> {
    let connection = open_app_db()?;
    let transaction = connection.unchecked_transaction().map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM snapshots WHERE repo_id = ?1", [repo_id])
        .map_err(|error| error.to_string())?;
    let mut statement = transaction
        .prepare(
            "
            INSERT INTO snapshots (snapshot_id, repo_id, time, hostname, label, tags_json, paths_json, summary_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
        )
        .map_err(|error| error.to_string())?;

    for snapshot in snapshots {
        statement
            .execute(params![
                snapshot.snapshot_id,
                snapshot.repo_id,
                snapshot.time,
                snapshot.hostname,
                snapshot.label,
                serde_json::to_string(&snapshot.tags).map_err(|error| error.to_string())?,
                serde_json::to_string(&snapshot.paths).map_err(|error| error.to_string())?,
                snapshot
                    .summary
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|error| error.to_string())?,
            ])
            .map_err(|error| error.to_string())?;
    }

    drop(statement);
    transaction.commit().map_err(|error| error.to_string())
}

fn explorer_cache_exists(repo_id: &str, snapshot_id: &str, dir_path: &str) -> Result<bool, String> {
    let connection = open_app_db()?;
    connection
        .query_row(
            "
            SELECT EXISTS(
                SELECT 1 FROM snapshot_entries_cache
                WHERE repo_id = ?1 AND snapshot_id = ?2 AND dir_path = ?3
            )
            ",
            params![repo_id, snapshot_id, dir_path],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(|error| error.to_string())
}

fn replace_snapshot_entries_cache(
    repo_id: &str,
    snapshot_id: &str,
    dir_path: &str,
    entries: &[VaultEntry],
    listed_at: &str,
) -> Result<(), String> {
    let connection = open_app_db()?;
    let transaction = connection.unchecked_transaction().map_err(|error| error.to_string())?;
    transaction
        .execute(
            "
            DELETE FROM snapshot_entries_cache
            WHERE repo_id = ?1 AND snapshot_id = ?2 AND dir_path = ?3
            ",
            params![repo_id, snapshot_id, dir_path],
        )
        .map_err(|error| error.to_string())?;

    let mut statement = transaction
        .prepare(
            "
            INSERT INTO snapshot_entries_cache (
                repo_id, snapshot_id, dir_path, entry_path, name, is_dir, size, mod_time, mime_type, listed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
        )
        .map_err(|error| error.to_string())?;

    if entries.is_empty() {
        statement
            .execute(params![
                repo_id,
                snapshot_id,
                dir_path,
                "",
                "",
                0_i64,
                0_i64,
                Option::<String>::None,
                Option::<String>::None,
                listed_at
            ])
            .map_err(|error| error.to_string())?;
    } else {
        for entry in entries {
            statement
                .execute(params![
                    repo_id,
                    snapshot_id,
                    dir_path,
                    entry.path,
                    entry.name,
                    if entry.is_dir { 1_i64 } else { 0_i64 },
                    entry.size as i64,
                    entry.mod_time,
                    entry.mime_type,
                    listed_at,
                ])
                .map_err(|error| error.to_string())?;
            }
    }

    drop(statement);
    transaction.commit().map_err(|error| error.to_string())
}

fn query_snapshot_entries_page(
    repo_id: &str,
    snapshot_id: &str,
    dir_path: &str,
    query: &str,
    offset: u64,
    limit: u64,
) -> Result<(Vec<VaultEntry>, u64, Option<String>), String> {
    let connection = open_app_db()?;
    let listed_at = connection
        .query_row(
            "
            SELECT listed_at
            FROM snapshot_entries_cache
            WHERE repo_id = ?1 AND snapshot_id = ?2 AND dir_path = ?3
            ORDER BY listed_at DESC
            LIMIT 1
            ",
            params![repo_id, snapshot_id, dir_path],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let like_query = format!("%{}%", query.trim());
    let filter_active = !query.trim().is_empty();

    let total_count: u64 = if filter_active {
        connection
            .query_row(
                "
                SELECT COUNT(*)
                FROM snapshot_entries_cache
                WHERE repo_id = ?1 AND snapshot_id = ?2 AND dir_path = ?3 AND entry_path <> ''
                  AND name LIKE ?4
                ",
                params![repo_id, snapshot_id, dir_path, like_query],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())? as u64
    } else {
        connection
            .query_row(
                "
                SELECT COUNT(*)
                FROM snapshot_entries_cache
                WHERE repo_id = ?1 AND snapshot_id = ?2 AND dir_path = ?3 AND entry_path <> ''
                ",
                params![repo_id, snapshot_id, dir_path],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())? as u64
    };

    let mut statement = if filter_active {
        connection
            .prepare(
                "
                SELECT name, entry_path, is_dir, size, mod_time, mime_type
                FROM snapshot_entries_cache
                WHERE repo_id = ?1 AND snapshot_id = ?2 AND dir_path = ?3 AND entry_path <> ''
                  AND name LIKE ?4
                ORDER BY is_dir DESC, LOWER(name) ASC
                LIMIT ?5 OFFSET ?6
                ",
            )
            .map_err(|error| error.to_string())?
    } else {
        connection
            .prepare(
                "
                SELECT name, entry_path, is_dir, size, mod_time, mime_type
                FROM snapshot_entries_cache
                WHERE repo_id = ?1 AND snapshot_id = ?2 AND dir_path = ?3 AND entry_path <> ''
                ORDER BY is_dir DESC, LOWER(name) ASC
                LIMIT ?4 OFFSET ?5
                ",
            )
            .map_err(|error| error.to_string())?
    };

    let mapper = |row: &rusqlite::Row<'_>| -> rusqlite::Result<VaultEntry> {
        let name: String = row.get("name")?;
        Ok(VaultEntry {
            display_name: name.clone(),
            name,
            path: row.get("entry_path")?,
            is_dir: row.get::<_, i64>("is_dir")? != 0,
            size: row.get::<_, i64>("size")? as u64,
            mod_time: row.get("mod_time")?,
            mime_type: row.get("mime_type")?,
        })
    };

    let rows = if filter_active {
        statement
            .query_map(
                params![repo_id, snapshot_id, dir_path, like_query, limit as i64, offset as i64],
                mapper,
            )
            .map_err(|error| error.to_string())?
    } else {
        statement
            .query_map(params![repo_id, snapshot_id, dir_path, limit as i64, offset as i64], mapper)
            .map_err(|error| error.to_string())?
    };

    Ok((
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())?,
        total_count,
        listed_at,
    ))
}

fn read_snapshots_from_repository(repo_id: &str, provider: &str) -> Result<Vec<VaultSnapshotInfo>, String> {
    let password = require_vault_password(repo_id)?;
    let repo = open_or_init_repository(provider, &password, NoProgressBars)?;
    let mut snapshots = repo
        .get_all_snapshots()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|snapshot| snapshot_info_from_snapshot(repo_id, &snapshot))
        .collect::<Vec<_>>();
    snapshots.sort_by(|left, right| right.time.cmp(&left.time));
    replace_snapshots_cache(repo_id, &snapshots)?;
    let latest = snapshots.first().map(|snapshot| snapshot.time.as_str());
    let _ = upsert_vault_repository(provider, latest)?;
    Ok(snapshots)
}

fn classify_preview_kind(name: &str, mime_type: Option<&str>) -> &'static str {
    if let Some(mime_type) = mime_type {
        if mime_type.starts_with("image/") {
            return "image";
        }
        if mime_type.starts_with("text/") {
            return "text";
        }
    }

    match Path::new(name)
        .extension()
        .and_then(OsStr::to_str)
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp") => "image",
        Some(
            "txt"
            | "md"
            | "markdown"
            | "json"
            | "js"
            | "ts"
            | "tsx"
            | "jsx"
            | "css"
            | "html"
            | "xml"
            | "csv"
            | "log"
            | "toml"
            | "yaml"
            | "yml"
            | "rs"
            | "py"
            | "sh",
        ) => "text",
        _ => "unsupported",
    }
}

fn guess_image_mime_type(name: &str, mime_type: Option<&str>) -> String {
    if let Some(mime_type) = mime_type.filter(|value| value.starts_with("image/")) {
        return mime_type.to_string();
    }
    match Path::new(name)
        .extension()
        .and_then(OsStr::to_str)
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png".to_string(),
        Some("jpg" | "jpeg") => "image/jpeg".to_string(),
        Some("gif") => "image/gif".to_string(),
        Some("webp") => "image/webp".to_string(),
        Some("svg") => "image/svg+xml".to_string(),
        Some("bmp") => "image/bmp".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

fn downloads_dir() -> Result<PathBuf, String> {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .ok_or_else(|| "Downloads directory could not be resolved".to_string())
}

fn unique_download_path(base_dir: &Path, name: &str) -> PathBuf {
    let candidate = base_dir.join(name);
    if !candidate.exists() {
        return candidate;
    }

    let stem = Path::new(name)
        .file_stem()
        .and_then(OsStr::to_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("download");
    let extension = Path::new(name)
        .extension()
        .and_then(OsStr::to_str)
        .filter(|value| !value.is_empty());

    for index in 1..1000 {
        let file_name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = base_dir.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    base_dir.join(format!("{stem}-download"))
}

fn next_job_identifiers() -> (String, String) {
    let sequence = JOB_COUNTER.fetch_add(1, Ordering::Relaxed);
    (format!("job-{sequence}"), format!("exec-{sequence}"))
}

fn initial_job_progress(display_name: &str) -> JobProgress {
    JobProgress {
        bytes_done: 0,
        bytes_total: None,
        speed: None,
        eta: None,
        current_file: Some(display_name.to_string()),
        transfers: None,
    }
}

fn create_job_record(
    job_id: String,
    execute_id: String,
    kind: &str,
    display_name: String,
    source_ref: String,
    target_ref: String,
    provider: Option<String>,
    repo_id: Option<String>,
) -> Result<JobRecord, String> {
    Ok(JobRecord {
        job_id,
        execute_id,
        kind: kind.to_string(),
        phase: "running".to_string(),
        provider,
        repo_id,
        source_ref,
        target_ref,
        display_name: display_name.clone(),
        progress: initial_job_progress(&display_name),
        error: None,
        result: None,
        started_at: Some(upload_timestamp()?),
        finished_at: None,
    })
}

fn job_status_from_record(record: &JobRecord) -> JobStatus {
    JobStatus {
        job_id: record.job_id.clone(),
        execute_id: record.execute_id.clone(),
        kind: record.kind.clone(),
        phase: record.phase.clone(),
        progress: record.progress.clone(),
        error: record.error.clone(),
        result: record.result.clone(),
        started_at: record.started_at.clone(),
        finished_at: record.finished_at.clone(),
    }
}

fn persist_job_record(record: &JobRecord) -> Result<(), String> {
    let connection = open_app_db()?;
    let result_json = if record.kind == "preview" {
        None
    } else {
        record
            .result
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| error.to_string())?
    };

    connection
        .execute(
            "
            INSERT OR REPLACE INTO transfer_jobs (
                job_id, kind, status, execute_id, provider, repo_id,
                source_ref, target_ref, display_name, bytes_done, bytes_total,
                speed, eta, current_item, error, started_at, finished_at, result_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
            ",
            params![
                record.job_id,
                record.kind,
                record.phase,
                record.execute_id,
                record.provider,
                record.repo_id,
                record.source_ref,
                record.target_ref,
                record.display_name,
                record.progress.bytes_done as i64,
                record.progress.bytes_total.map(|value| value as i64),
                record.progress.speed.map(|value| value as i64),
                record.progress.eta.map(|value| value as i64),
                record.progress.current_file,
                record.error,
                record.started_at,
                record.finished_at,
                result_json,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn insert_or_update_job_record(record: JobRecord) -> Result<JobRecord, String> {
    persist_job_record(&record)?;
    jobs()
        .lock()
        .map_err(|_| "job registry unavailable".to_string())?
        .insert(record.job_id.clone(), record.clone());
    Ok(record)
}

fn update_job_record<F>(job_id: &str, mutator: F) -> Result<JobRecord, String>
where
    F: FnOnce(&mut JobRecord),
{
    let updated = {
        let mut registry = jobs()
            .lock()
            .map_err(|_| "job registry unavailable".to_string())?;
        let record = registry
            .get_mut(job_id)
            .ok_or_else(|| format!("unknown job: {job_id}"))?;
        mutator(record);
        record.clone()
    };
    persist_job_record(&updated)?;
    Ok(updated)
}

fn read_job_record_from_db(job_id: &str) -> Result<JobRecord, String> {
    let connection = open_app_db()?;
    connection
        .query_row(
            "
            SELECT job_id, kind, status, execute_id, provider, repo_id, source_ref, target_ref,
                   display_name, bytes_done, bytes_total, speed, eta, current_item, error,
                   started_at, finished_at, result_json
            FROM transfer_jobs
            WHERE job_id = ?1
            ",
            [job_id],
            |row| {
                let result_json: Option<String> = row.get("result_json")?;
                Ok(JobRecord {
                    job_id: row.get("job_id")?,
                    execute_id: row.get("execute_id")?,
                    kind: row.get("kind")?,
                    phase: row.get("status")?,
                    provider: row.get("provider")?,
                    repo_id: row.get("repo_id")?,
                    source_ref: row.get("source_ref")?,
                    target_ref: row.get("target_ref")?,
                    display_name: row.get("display_name")?,
                    progress: JobProgress {
                        bytes_done: row.get::<_, i64>("bytes_done")? as u64,
                        bytes_total: row.get::<_, Option<i64>>("bytes_total")?.map(|value| value as u64),
                        speed: row.get::<_, Option<i64>>("speed")?.map(|value| value as u64),
                        eta: row.get::<_, Option<i64>>("eta")?.map(|value| value as u64),
                        current_file: row.get("current_item")?,
                        transfers: None,
                    },
                    error: row.get("error")?,
                    result: result_json
                        .as_deref()
                        .map(serde_json::from_str)
                        .transpose()
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                    started_at: row.get("started_at")?,
                    finished_at: row.get("finished_at")?,
                })
            },
        )
        .map_err(|error| format!("unknown job `{job_id}`: {error}"))
}

fn flush_rustic_progress(job_id: &str, state: &RusticJobSharedState) {
    let _ = update_job_record(job_id, |record| {
        record.progress.bytes_done = state.bytes_done;
        record.progress.bytes_total = state.bytes_total;
        record.progress.current_file = state.current_file.clone();
        record.progress.speed = None;
        record.progress.eta = None;
    });
}

impl ProgressBars for RusticJobProgressBars {
    fn progress(&self, progress_type: ProgressType, _prefix: &str) -> Progress {
        Progress::new(RusticJobProgress {
            job_id: self.job_id.clone(),
            state: self.state.clone(),
            progress_type,
        })
    }
}

impl RusticProgress for RusticJobProgress {
    fn is_hidden(&self) -> bool {
        false
    }

    fn set_length(&self, len: u64) {
        if let Ok(mut state) = self.state.lock() {
            if matches!(self.progress_type, ProgressType::Bytes) {
                state.bytes_total = Some(len);
            }
            if state.last_flush.elapsed() >= Duration::from_millis(250) {
                state.last_flush = Instant::now();
                flush_rustic_progress(&self.job_id, &state);
            }
        }
    }

    fn set_title(&self, title: &str) {
        if let Ok(mut state) = self.state.lock() {
            if !title.trim().is_empty() {
                state.current_file = Some(title.to_string());
            }
            if state.last_flush.elapsed() >= Duration::from_millis(250) {
                state.last_flush = Instant::now();
                flush_rustic_progress(&self.job_id, &state);
            }
        }
    }

    fn inc(&self, inc: u64) {
        if let Ok(mut state) = self.state.lock() {
            if matches!(self.progress_type, ProgressType::Bytes) {
                state.bytes_done = state.bytes_done.saturating_add(inc);
            }
            if state.last_flush.elapsed() >= Duration::from_millis(250) {
                state.last_flush = Instant::now();
                flush_rustic_progress(&self.job_id, &state);
            }
        }
    }

    fn finish(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.last_flush = Instant::now();
            flush_rustic_progress(&self.job_id, &state);
        }
    }
}

fn progress_bars_for_job(job_id: &str) -> RusticJobProgressBars {
    RusticJobProgressBars {
        job_id: job_id.to_string(),
        state: Arc::new(Mutex::new(RusticJobSharedState {
            bytes_done: 0,
            bytes_total: None,
            current_file: None,
            last_flush: Instant::now(),
        })),
    }
}

#[tauri::command]
fn ping(payload: Option<PingPayload>) -> PingResponse {
    let context = guest_context();
    PingResponse {
        ok: true,
        adapter: context.adapter().to_string(),
        session_id: context.session_id().to_string(),
        echo: payload.and_then(|value| value.message).unwrap_or_default(),
    }
}

#[tauri::command]
fn check_env() -> CheckEnvResponse {
    let context = guest_context();
    CheckEnvResponse {
        ok: true,
        adapter: context.adapter().to_string(),
        session_id: context.session_id().to_string(),
        ato_guest_mode: context.guest_mode().map(str::to_string),
    }
}

#[tauri::command]
fn get_runtime_config() -> RuntimeConfigResponse {
    runtime_config()
}

#[tauri::command]
fn get_providers() -> ProvidersResponse {
    ProvidersResponse {
        providers: vec![
            ProviderInfo {
                id: "drive".to_string(),
                label: "Google Drive".to_string(),
                auth_kind: "oauth".to_string(),
            },
            ProviderInfo {
                id: "r2".to_string(),
                label: "Cloudflare R2".to_string(),
                auth_kind: "access_key".to_string(),
            },
        ],
    }
}

#[tauri::command]
fn get_provider_statuses() -> ProviderStatusesResponse {
    let drive_connected = has_rclone_section(&app_config().default_gdrive_remote).unwrap_or(false);
    let r2_connected = resolve_r2_config().is_ok();
    ProviderStatusesResponse {
        providers: vec![
            ProviderStatusInfo {
                id: "drive".to_string(),
                connected: drive_connected,
            },
            ProviderStatusInfo {
                id: "r2".to_string(),
                connected: r2_connected,
            },
        ],
    }
}

#[tauri::command]
fn connect_provider(payload: ConnectProviderRequest) -> ConnectProviderResponse {
    let config_path = app_config().config_path.display().to_string();

    match payload.provider.as_str() {
        "drive" => connect_google_drive(config_path),
        "r2" => match resolve_r2_config() {
            Ok(_) => ConnectProviderResponse {
                ok: true,
                provider: "r2".to_string(),
                status: "configured".to_string(),
                next_action: "R2 credentials loaded from .env".to_string(),
                config_path,
            },
            Err(error) => ConnectProviderResponse {
                ok: false,
                provider: "r2".to_string(),
                status: "missing_env".to_string(),
                next_action: format!(
                    "{error}. Set R2_BUCKET / R2_ENDPOINT(or R2_ACCOUNT_ID) / R2_ACCESS_KEY_ID / R2_SECRET_ACCESS_KEY in .env"
                ),
                config_path,
            },
        },
        _ => ConnectProviderResponse {
            ok: false,
            provider: payload.provider,
            status: "unsupported".to_string(),
            next_action: "Provider is not implemented yet".to_string(),
            config_path,
        },
    }
}

#[tauri::command]
fn init_vault_repository(
    payload: InitVaultRepositoryRequest,
) -> Result<InitVaultRepositoryResponse, String> {
    let repo_id = repo_id_for_provider(&payload.provider)?;
    if payload.password.trim().is_empty() {
        return Err("Repository password is required".to_string());
    }
    let use_keychain = payload.use_keychain.unwrap_or(app_config().use_keychain);
    store_vault_password(&repo_id, &payload.password, use_keychain)?;
    let _ = open_or_init_repository(&payload.provider, &payload.password, NoProgressBars)?;
    let _ = upsert_vault_repository(&payload.provider, None)?;
    Ok(InitVaultRepositoryResponse { repo_id })
}

#[tauri::command]
fn start_vault_backup(
    payload: StartVaultBackupRequest,
) -> Result<StartVaultBackupResponse, String> {
    let repo_id = repo_id_for_provider(&payload.provider)?;
    let password = require_vault_password(&repo_id)?;
    let source_paths = payload
        .source_paths
        .iter()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if source_paths.is_empty() {
        return Err("No source paths were provided".to_string());
    }

    let (job_id, execute_id) = next_job_identifiers();
    let display_name = if source_paths.len() == 1 {
        path_basename(&source_paths[0])
    } else {
        format!("{} items", source_paths.len())
    };
    let record = create_job_record(
        job_id.clone(),
        execute_id.clone(),
        "upload",
        display_name,
        source_paths.join("\n"),
        repo_id.clone(),
        Some(payload.provider.clone()),
        Some(repo_id.clone()),
    )?;
    insert_or_update_job_record(record)?;

    let job_id_for_thread = job_id.clone();
    thread::spawn(move || {
        let progress = progress_bars_for_job(&job_id_for_thread);
        let outcome = (|| -> Result<Value, String> {
            let repo = open_indexed_ids_repository(&payload.provider, &password, progress)?;
            let path_list = PathList::from_iter(source_paths.clone()).sanitize().map_err(|error| error.to_string())?;
            let mut snapshot_opts = SnapshotOptions::default();
            snapshot_opts.label = Some("Capsuled Vault Backup".to_string());
            let snapshot = repo
                .backup(
                    &BackupOptions::default(),
                    &path_list,
                    snapshot_opts.to_snapshot().map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;

            let snapshot_info = snapshot_info_from_snapshot(&repo_id, &snapshot);
            replace_snapshots_cache(&repo_id, std::slice::from_ref(&snapshot_info))?;
            let _ = upsert_vault_repository(&payload.provider, Some(&snapshot_info.time))?;

            let summary = snapshot.summary.unwrap_or_default();
            Ok(json!(BackupJobResult {
                snapshot_id: snapshot.id.to_string(),
                files_new: summary.files_new,
                files_changed: summary.files_changed,
                files_unchanged: summary.files_unmodified,
                dirs_new: summary.dirs_new,
                total_bytes_processed: summary.total_bytes_processed,
                total_bytes_added: summary.data_added_packed,
            }))
        })();

        match outcome {
            Ok(result) => {
                let _ = update_job_record(&job_id_for_thread, |record| {
                    record.phase = "done".to_string();
                    record.finished_at = upload_timestamp().ok();
                    record.progress.speed = None;
                    record.progress.eta = None;
                    if record.progress.bytes_total.is_none() && record.progress.bytes_done > 0 {
                        record.progress.bytes_total = Some(record.progress.bytes_done);
                    }
                    record.result = Some(result);
                });
            }
            Err(error) => {
                let _ = update_job_record(&job_id_for_thread, |record| {
                    record.phase = "failed".to_string();
                    record.error = Some(error);
                    record.finished_at = upload_timestamp().ok();
                    record.progress.speed = None;
                    record.progress.eta = None;
                });
            }
        }
    });

    Ok(StartVaultBackupResponse { job_id, execute_id })
}

#[tauri::command]
fn list_vault_repositories() -> Result<ListVaultRepositoriesResponse, String> {
    Ok(ListVaultRepositoriesResponse {
        repositories: list_vault_repository_rows()?,
    })
}

#[tauri::command]
fn list_vault_snapshots(
    payload: ListVaultSnapshotsRequest,
) -> Result<ListVaultSnapshotsResponse, String> {
    let repo = read_vault_repository(&payload.repo_id)?;
    let limit = payload.limit.unwrap_or(100).clamp(1, 500) as usize;
    let snapshots = read_snapshots_from_repository(&repo.repo_id, &repo.provider)?;
    Ok(ListVaultSnapshotsResponse {
        snapshots: snapshots.into_iter().take(limit).collect(),
    })
}

#[tauri::command]
fn list_vault_entries(
    payload: ListVaultEntriesRequest,
) -> Result<ListVaultEntriesResponse, String> {
    let repository = read_vault_repository(&payload.repo_id)?;
    let password = require_vault_password(&repository.repo_id)?;
    let current_path = sanitize_relative_path(payload.path.as_deref())?;
    let query = payload.query.unwrap_or_default();
    let offset = payload.offset.unwrap_or(0);
    let limit = payload.limit.unwrap_or(200).clamp(1, 500);
    let refresh = payload.refresh.unwrap_or(false);

    let repo = open_indexed_full_repository(&repository.provider, &password)?;
    let snapshot = repo
        .get_snapshot_from_str(&payload.snapshot_id, |_| true)
        .map_err(|error| error.to_string())?;
    let snapshot_info = snapshot_info_from_snapshot(&repository.repo_id, &snapshot);

    if refresh || !explorer_cache_exists(&repository.repo_id, &snapshot_info.snapshot_id, &current_path)? {
        let node = repo
            .node_from_snapshot_and_path(&snapshot, &current_path)
            .map_err(|error| error.to_string())?;
        let entries = repo
            .ls(&node, &LsOptions::default())
            .map_err(|error| error.to_string())?
            .map(|item| {
                let (child_path, child_node) = item.map_err(|error| error.to_string())?;
                let child_name = if child_path.as_os_str().is_empty() {
                    child_node.name().to_string_lossy().to_string()
                } else {
                    child_path.to_string_lossy().to_string()
                };
                let full_path = join_relative_path(&current_path, &child_name);
                Ok(VaultEntry {
                    name: path_basename(&child_name),
                    display_name: path_basename(&child_name),
                    path: full_path,
                    is_dir: child_node.is_dir(),
                    size: child_node.meta.size,
                    mod_time: child_node.meta.mtime.map(|value| value.to_string()),
                    mime_type: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let listed_at = upload_timestamp()?;
        replace_snapshot_entries_cache(
            &repository.repo_id,
            &snapshot_info.snapshot_id,
            &current_path,
            &entries,
            &listed_at,
        )?;
    }

    let (entries, total_count, listed_at) = query_snapshot_entries_page(
        &repository.repo_id,
        &snapshot_info.snapshot_id,
        &current_path,
        &query,
        offset,
        limit,
    )?;

    Ok(ListVaultEntriesResponse {
        repository,
        snapshot: snapshot_info,
        current_path,
        total_count,
        next_offset: ((offset + entries.len() as u64) < total_count).then_some(offset + entries.len() as u64),
        entries,
        listed_at,
    })
}

#[tauri::command]
fn start_vault_preview(payload: VaultItemRequest) -> Result<StartJobResponse, String> {
    let repository = read_vault_repository(&payload.repo_id)?;
    let relative_path = sanitize_relative_path(payload.path.as_deref())?;
    if relative_path.is_empty() {
        return Err("Folders cannot be previewed".to_string());
    }

    let (job_id, execute_id) = next_job_identifiers();
    let display_name = path_basename(&relative_path);
    let record = create_job_record(
        job_id.clone(),
        execute_id,
        "preview",
        display_name.clone(),
        relative_path.clone(),
        "preview".to_string(),
        Some(repository.provider.clone()),
        Some(repository.repo_id.clone()),
    )?;
    insert_or_update_job_record(record)?;

    let job_id_for_thread = job_id.clone();
    thread::spawn(move || {
        let outcome = (|| -> Result<Value, String> {
            let password = require_vault_password(&repository.repo_id)?;
            let repo = open_indexed_full_repository(&repository.provider, &password)?;
            let snapshot = repo
                .get_snapshot_from_str(&payload.snapshot_id, |_| true)
                .map_err(|error| error.to_string())?;
            let node = repo
                .node_from_snapshot_and_path(&snapshot, &relative_path)
                .map_err(|error| error.to_string())?;

            if node.is_dir() {
                return Err("Folders cannot be previewed".to_string());
            }

            let kind = classify_preview_kind(&display_name, None);
            match kind {
                "image" => {
                    if node.meta.size > 1024 * 1024 {
                        return Err("Image preview is limited to files up to 1 MB".to_string());
                    }
                    let file = repo.open_file(&node).map_err(|error| error.to_string())?;
                    let bytes = repo
                        .read_file_at(&file, 0, node.meta.size as usize)
                        .map_err(|error| error.to_string())?;
                    let mime_type = guess_image_mime_type(&display_name, None);
                    Ok(json!(PreviewExplorerItemResponse {
                        name: display_name.clone(),
                        path: relative_path,
                        mime_type: Some(mime_type.clone()),
                        kind: "image".to_string(),
                        text: Option::<String>::None,
                        image_data_url: Some(format!(
                            "data:{mime_type};base64,{}",
                            base64::engine::general_purpose::STANDARD.encode(bytes)
                        )),
                        truncated: false,
                        size: node.meta.size,
                    }))
                }
                "text" => {
                    let file = repo.open_file(&node).map_err(|error| error.to_string())?;
                    let limit = 64 * 1024;
                    let length = usize::min(limit, node.meta.size as usize);
                    let bytes = repo
                        .read_file_at(&file, 0, length)
                        .map_err(|error| error.to_string())?;
                    Ok(json!(PreviewExplorerItemResponse {
                        name: display_name.clone(),
                        path: relative_path,
                        mime_type: None::<String>,
                        kind: "text".to_string(),
                        text: Some(String::from_utf8_lossy(&bytes).to_string()),
                        image_data_url: Option::<String>::None,
                        truncated: node.meta.size > limit as u64,
                        size: node.meta.size,
                    }))
                }
                _ => Ok(json!(PreviewExplorerItemResponse {
                    name: display_name,
                    path: relative_path,
                    mime_type: None::<String>,
                    kind: "unsupported".to_string(),
                    text: Option::<String>::None,
                    image_data_url: Option::<String>::None,
                    truncated: false,
                    size: node.meta.size,
                })),
            }
        })();

        match outcome {
            Ok(result) => {
                let _ = update_job_record(&job_id_for_thread, |record| {
                    record.phase = "done".to_string();
                    record.finished_at = upload_timestamp().ok();
                    record.result = Some(result);
                });
            }
            Err(error) => {
                let _ = update_job_record(&job_id_for_thread, |record| {
                    record.phase = "failed".to_string();
                    record.error = Some(error);
                    record.finished_at = upload_timestamp().ok();
                });
            }
        }
    });

    Ok(StartJobResponse { job_id })
}

#[tauri::command]
fn start_vault_restore(payload: VaultItemRequest) -> Result<StartJobResponse, String> {
    let repository = read_vault_repository(&payload.repo_id)?;
    let relative_path = sanitize_relative_path(payload.path.as_deref())?;
    let (job_id, execute_id) = next_job_identifiers();
    let display_name = if relative_path.is_empty() {
        format!("snapshot-{}", payload.snapshot_id.chars().take(8).collect::<String>())
    } else {
        path_basename(&relative_path)
    };
    let record = create_job_record(
        job_id.clone(),
        execute_id,
        "download",
        display_name.clone(),
        relative_path.clone(),
        "downloads".to_string(),
        Some(repository.provider.clone()),
        Some(repository.repo_id.clone()),
    )?;
    insert_or_update_job_record(record)?;

    let job_id_for_thread = job_id.clone();
    thread::spawn(move || {
        let outcome = (|| -> Result<String, String> {
            let password = require_vault_password(&repository.repo_id)?;
            let repo = open_indexed_full_repository(&repository.provider, &password)?;
            let snapshot = repo
                .get_snapshot_from_str(&payload.snapshot_id, |_| true)
                .map_err(|error| error.to_string())?;
            let node = repo
                .node_from_snapshot_and_path(&snapshot, &relative_path)
                .map_err(|error| error.to_string())?;
            let downloads = downloads_dir()?;
            fs::create_dir_all(&downloads).map_err(|error| error.to_string())?;
            let target_path = unique_download_path(&downloads, &display_name);
            let destination = LocalDestination::new(
                &target_path.display().to_string(),
                true,
                !node.is_dir(),
            )
            .map_err(|error| error.to_string())?;
            let ls = repo
                .ls(&node, &LsOptions::default())
                .map_err(|error| error.to_string())?;
            let restore_plan = repo
                .prepare_restore(&RestoreOptions::default(), ls.clone(), &destination, false)
                .map_err(|error| error.to_string())?;
            repo.restore(restore_plan, &RestoreOptions::default(), ls, &destination)
                .map_err(|error| error.to_string())?;
            Ok(target_path.display().to_string())
        })();

        match outcome {
            Ok(saved_path) => {
                let _ = update_job_record(&job_id_for_thread, |record| {
                    record.phase = "done".to_string();
                    record.finished_at = upload_timestamp().ok();
                    record.result = Some(json!(DownloadExplorerItemResponse { saved_path }));
                });
            }
            Err(error) => {
                let _ = update_job_record(&job_id_for_thread, |record| {
                    record.phase = "failed".to_string();
                    record.error = Some(error);
                    record.finished_at = upload_timestamp().ok();
                });
            }
        }
    });

    Ok(StartJobResponse { job_id })
}

#[tauri::command]
fn list_jobs(payload: Option<ListJobsRequest>) -> Result<ListJobsResponse, String> {
    let payload = payload.unwrap_or(ListJobsRequest {
        kind: None,
        status: None,
        limit: None,
    });
    let limit = payload.limit.unwrap_or(50).clamp(1, 500);
    let connection = open_app_db()?;
    let mut statement = match (payload.kind.as_deref(), payload.status.as_deref()) {
        (Some(_), Some(_)) => connection
            .prepare(
                "
                SELECT job_id, kind, status, execute_id, provider, repo_id, source_ref, target_ref,
                       display_name, bytes_done, bytes_total, speed, eta, current_item, error,
                       started_at, finished_at, result_json
                FROM transfer_jobs
                WHERE kind = ?1 AND status = ?2
                ORDER BY COALESCE(started_at, finished_at) DESC
                LIMIT ?3
                ",
            )
            .map_err(|error| error.to_string())?,
        (Some(_), None) => connection
            .prepare(
                "
                SELECT job_id, kind, status, execute_id, provider, repo_id, source_ref, target_ref,
                       display_name, bytes_done, bytes_total, speed, eta, current_item, error,
                       started_at, finished_at, result_json
                FROM transfer_jobs
                WHERE kind = ?1
                ORDER BY COALESCE(started_at, finished_at) DESC
                LIMIT ?2
                ",
            )
            .map_err(|error| error.to_string())?,
        (None, Some(_)) => connection
            .prepare(
                "
                SELECT job_id, kind, status, execute_id, provider, repo_id, source_ref, target_ref,
                       display_name, bytes_done, bytes_total, speed, eta, current_item, error,
                       started_at, finished_at, result_json
                FROM transfer_jobs
                WHERE status = ?1
                ORDER BY COALESCE(started_at, finished_at) DESC
                LIMIT ?2
                ",
            )
            .map_err(|error| error.to_string())?,
        (None, None) => connection
            .prepare(
                "
                SELECT job_id, kind, status, execute_id, provider, repo_id, source_ref, target_ref,
                       display_name, bytes_done, bytes_total, speed, eta, current_item, error,
                       started_at, finished_at, result_json
                FROM transfer_jobs
                ORDER BY COALESCE(started_at, finished_at) DESC
                LIMIT ?1
                ",
            )
            .map_err(|error| error.to_string())?,
    };

    let mapper = |row: &rusqlite::Row<'_>| -> rusqlite::Result<JobStatus> {
        let result_json: Option<String> = row.get("result_json")?;
        Ok(JobStatus {
            job_id: row.get("job_id")?,
            execute_id: row.get("execute_id")?,
            kind: row.get("kind")?,
            phase: row.get("status")?,
            progress: JobProgress {
                bytes_done: row.get::<_, i64>("bytes_done")? as u64,
                bytes_total: row.get::<_, Option<i64>>("bytes_total")?.map(|value| value as u64),
                speed: row.get::<_, Option<i64>>("speed")?.map(|value| value as u64),
                eta: row.get::<_, Option<i64>>("eta")?.map(|value| value as u64),
                current_file: row.get("current_item")?,
                transfers: None,
            },
            error: row.get("error")?,
            result: result_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
            started_at: row.get("started_at")?,
            finished_at: row.get("finished_at")?,
        })
    };

    let rows = match (payload.kind.as_deref(), payload.status.as_deref()) {
        (Some(kind), Some(status)) => statement
            .query_map(params![kind, status, limit as i64], mapper)
            .map_err(|error| error.to_string())?,
        (Some(kind), None) => statement
            .query_map(params![kind, limit as i64], mapper)
            .map_err(|error| error.to_string())?,
        (None, Some(status)) => statement
            .query_map(params![status, limit as i64], mapper)
            .map_err(|error| error.to_string())?,
        (None, None) => statement
            .query_map(params![limit as i64], mapper)
            .map_err(|error| error.to_string())?,
    };

    Ok(ListJobsResponse {
        jobs: rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
fn get_job_status(payload: GetJobStatusRequest) -> Result<JobStatus, String> {
    if let Ok(registry) = jobs().lock() {
        if let Some(record) = registry.get(&payload.job_id) {
            return Ok(job_status_from_record(record));
        }
    }
    Ok(job_status_from_record(&read_job_record_from_db(&payload.job_id)?))
}

fn execute_command(context: &GuestContext, command: &str, envelope: CommandEnvelope) -> Value {
    if let Some(result) = builtin_result(context, command, &envelope) {
        return result;
    }

    match command {
        "get_runtime_config" => json!(runtime_config()),
        "get_providers" => json!(get_providers()),
        "get_provider_statuses" => json!(get_provider_statuses()),
        "connect_provider" => match serde_json::from_value::<ConnectProviderRequest>(envelope.payload) {
            Ok(payload) => json!(connect_provider(payload)),
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        },
        "init_vault_repository" => match serde_json::from_value::<InitVaultRepositoryRequest>(envelope.payload) {
            Ok(payload) => match init_vault_repository(payload) {
                Ok(response) => json!(response),
                Err(error) => json!({ "ok": false, "error": error }),
            },
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        },
        "start_vault_backup" => match serde_json::from_value::<StartVaultBackupRequest>(envelope.payload) {
            Ok(payload) => match start_vault_backup(payload) {
                Ok(response) => json!(response),
                Err(error) => json!({ "ok": false, "error": error }),
            },
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        },
        "list_vault_repositories" => match list_vault_repositories() {
            Ok(response) => json!(response),
            Err(error) => json!({ "ok": false, "error": error }),
        },
        "list_vault_snapshots" => match serde_json::from_value::<ListVaultSnapshotsRequest>(envelope.payload) {
            Ok(payload) => match list_vault_snapshots(payload) {
                Ok(response) => json!(response),
                Err(error) => json!({ "ok": false, "error": error }),
            },
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        },
        "list_vault_entries" => match serde_json::from_value::<ListVaultEntriesRequest>(envelope.payload) {
            Ok(payload) => match list_vault_entries(payload) {
                Ok(response) => json!(response),
                Err(error) => json!({ "ok": false, "error": error }),
            },
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        },
        "start_vault_restore" => match serde_json::from_value::<VaultItemRequest>(envelope.payload) {
            Ok(payload) => match start_vault_restore(payload) {
                Ok(response) => json!(response),
                Err(error) => json!({ "ok": false, "error": error }),
            },
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        },
        "start_vault_preview" => match serde_json::from_value::<VaultItemRequest>(envelope.payload) {
            Ok(payload) => match start_vault_preview(payload) {
                Ok(response) => json!(response),
                Err(error) => json!({ "ok": false, "error": error }),
            },
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        },
        "list_jobs" => match serde_json::from_value::<Option<ListJobsRequest>>(envelope.payload) {
            Ok(payload) => match list_jobs(payload) {
                Ok(response) => json!(response),
                Err(error) => json!({ "ok": false, "error": error }),
            },
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        },
        "get_job_status" => match serde_json::from_value::<GetJobStatusRequest>(envelope.payload) {
            Ok(payload) => match get_job_status(payload) {
                Ok(response) => json!(response),
                Err(error) => json!({ "ok": false, "error": error }),
            },
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        },
        _ => json!({
            "ok": false,
            "error": format!("unknown command: {command}"),
        }),
    }
}

pub fn run_guest_server() -> Result<(), String> {
    let context = guest_context();
    serve_guest_http(&context, execute_command)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ping,
            check_env,
            get_runtime_config,
            get_providers,
            get_provider_statuses,
            connect_provider,
            init_vault_repository,
            start_vault_backup,
            list_vault_repositories,
            list_vault_snapshots,
            list_vault_entries,
            start_vault_restore,
            start_vault_preview,
            list_jobs,
            get_job_status
        ])
        .plugin(tauri_plugin_dialog::init())
        .build(tauri::generate_context!())
        .expect("error while building byok-encrypted-r2-drop")
        .run(|_app_handle, _event| {});
}
