use base64::Engine;
use desky_guest_tauri::{builtin_result, serve_guest_http, CommandEnvelope, GuestContext};
use dotenvy::from_path_override;
use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use tauri_plugin_dialog::DialogExt;
use tiny_http::{Response, Server};
use time::format_description::well_known::Rfc3339;
use time::Duration as TimeDuration;
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCryptRemoteRequest {
    base_remote: String,
    crypt_suffix: String,
    remote_root_path: Option<String>,
    password: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateCryptRemoteResponse {
    ok: bool,
    base_remote: String,
    crypt_remote: String,
    config_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartUploadRequest {
    source_path: String,
    remote_name: String,
    remote_path: String,
    mode: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartUploadResponse {
    job_id: String,
    execute_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UploadIndexEntry {
    upload_id: String,
    uploaded_at: String,
    provider: String,
    view_base_remote: String,
    view_crypt_remote: String,
    source_path: String,
    remote_root_path: String,
    remote_item_path: String,
    item_type: String,
    display_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListUploadIndexResponse {
    uploads: Vec<UploadIndexEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FindUploadIndexEntryRequest {
    provider: String,
    remote_item_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FindUploadIndexEntryResponse {
    entry: Option<UploadIndexEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListExplorerEntriesRequest {
    upload_id: String,
    path: Option<String>,
    mode: String,
    query: Option<String>,
    offset: Option<u64>,
    limit: Option<u64>,
    refresh: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExplorerEntry {
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
struct ListExplorerEntriesResponse {
    upload: UploadIndexEntry,
    current_path: String,
    total_count: u64,
    next_offset: Option<u64>,
    entries: Vec<ExplorerEntry>,
    listed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExplorerItemRequest {
    upload_id: String,
    path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartJobResponse {
    job_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadExplorerItemResponse {
    saved_path: String,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PickFolderResponse {
    path: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct JobProgress {
    bytes_done: u64,
    bytes_total: Option<u64>,
    speed: Option<u64>,
    eta: Option<u64>,
    current_file: Option<String>,
    transfers: Option<u64>,
}

#[derive(Debug, Serialize, Clone)]
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
    kind: String,
    execute_id: String,
    display_name: String,
    source_ref: String,
    target_ref: String,
    provider: Option<String>,
    upload_id: Option<String>,
    phase: String,
    progress: JobProgress,
    error: Option<String>,
    result: Option<Value>,
    started_at: Option<String>,
    finished_at: Option<String>,
    pending_upload: Option<PendingUploadRecord>,
}

#[derive(Debug, Clone)]
struct PendingUploadRecord {
    upload_id: String,
    provider: String,
    view_base_remote: String,
    view_crypt_remote: String,
    source_path: String,
    remote_root_path: String,
    remote_item_path: String,
    item_type: String,
    display_name: String,
}

#[derive(Debug, Clone)]
struct AppConfig {
    rclone_binary: String,
    rc_addr: String,
    rc_user: String,
    rc_pass: String,
    config_path: PathBuf,
    state_dir: PathBuf,
    use_keychain: bool,
    default_mode: String,
    job_poll_interval_ms: u64,
    default_gdrive_remote: String,
    google_drive_client_id: Option<String>,
    google_drive_client_secret: Option<String>,
    google_drive_callback_port: u16,
    google_drive_scopes: String,
    default_r2_remote: String,
    default_crypt_remote_suffix: String,
}

#[derive(Debug)]
struct GoogleDriveOAuthConfig {
    client_id: String,
    client_secret: String,
    callback_port: u16,
    scopes: String,
}

#[derive(Default)]
struct SidecarState {
    child: Option<Child>,
}

#[derive(Debug, Deserialize)]
struct RcJobStartResponse {
    #[serde(rename = "jobid")]
    job_id: u64,
}

#[derive(Debug, Deserialize)]
struct RcJobStatusResponse {
    finished: Option<bool>,
    success: Option<bool>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RcStatsResponse {
    #[serde(default)]
    bytes: u64,
    #[serde(default)]
    total_bytes: u64,
    #[serde(default)]
    speed: f64,
    eta: Option<f64>,
    #[serde(default)]
    transferring: Vec<RcTransferring>,
}

#[derive(Debug, Deserialize)]
struct RcTransferring {
    name: Option<String>,
    bytes: Option<u64>,
    size: Option<u64>,
    speed: Option<f64>,
    eta: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RcloneLsJsonItem {
    name: String,
    path: String,
    #[serde(default)]
    is_dir: bool,
    #[serde(default, deserialize_with = "deserialize_rclone_size")]
    size: u64,
    mod_time: Option<String>,
    mime_type: Option<String>,
    encrypted: Option<String>,
}

fn deserialize_rclone_size<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<JsonValue>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(0);
    };

    match value {
        JsonValue::Number(number) => {
            if let Some(size) = number.as_u64() {
                Ok(size)
            } else if let Some(size) = number.as_i64() {
                Ok(size.max(0) as u64)
            } else {
                Ok(0)
            }
        }
        _ => Ok(0),
    }
}

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);
static JOBS: OnceLock<Mutex<HashMap<String, JobRecord>>> = OnceLock::new();
static APP_CONFIG: OnceLock<AppConfig> = OnceLock::new();
static SIDECAR_STATE: OnceLock<Mutex<SidecarState>> = OnceLock::new();
static SIDECAR_STARTUP_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static SIDECAR_RUNTIME_ADDR: OnceLock<Mutex<String>> = OnceLock::new();
static DB_STARTUP_STATE: OnceLock<()> = OnceLock::new();
static DOTENV_STATE: OnceLock<()> = OnceLock::new();

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

fn sidecar_state() -> &'static Mutex<SidecarState> {
    SIDECAR_STATE.get_or_init(|| Mutex::new(SidecarState::default()))
}

fn sidecar_startup_lock() -> &'static Mutex<()> {
    SIDECAR_STARTUP_LOCK.get_or_init(|| Mutex::new(()))
}

fn sidecar_runtime_addr() -> &'static Mutex<String> {
    SIDECAR_RUNTIME_ADDR.get_or_init(|| Mutex::new(app_config().rc_addr.clone()))
}

fn current_rc_addr() -> String {
    sidecar_runtime_addr()
        .lock()
        .map(|addr| addr.clone())
        .unwrap_or_else(|_| app_config().rc_addr.clone())
}

fn set_current_rc_addr(rc_addr: String) -> Result<(), String> {
    *sidecar_runtime_addr()
        .lock()
        .map_err(|_| "sidecar runtime addr unavailable".to_string())? = rc_addr;
    Ok(())
}

fn normalize_error(error: Option<String>) -> Option<String> {
    error.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn eta_seconds(value: Option<f64>) -> Option<u64> {
    value.and_then(|eta| (eta.is_finite() && eta >= 0.0).then_some(eta as u64))
}

fn upload_timestamp() -> Result<String, String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| error.to_string())
}

fn infer_base_remote(remote_name: &str) -> String {
    let suffix = &app_config().default_crypt_remote_suffix;
    if remote_name.ends_with(suffix) && remote_name.len() > suffix.len() {
        remote_name[..remote_name.len() - suffix.len()].to_string()
    } else {
        remote_name.to_string()
    }
}

fn provider_from_base_remote(base_remote: &str) -> String {
    let config = app_config();
    if base_remote == config.default_gdrive_remote {
        "drive".to_string()
    } else if base_remote == config.default_r2_remote {
        "r2".to_string()
    } else {
        base_remote.to_string()
    }
}

fn build_pending_upload_record(
    execute_id: &str,
    source_path: &Path,
    remote_name: &str,
    remote_path: &str,
) -> PendingUploadRecord {
    let base_remote = infer_base_remote(remote_name);
    let item_type = if source_path.is_file() {
        "file"
    } else {
        "directory"
    };
    let display_name = source_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| source_path.display().to_string());
    let remote_root_path = remote_path.trim_matches('/').to_string();
    let remote_item_path = display_name.clone();

    PendingUploadRecord {
        upload_id: execute_id.to_string(),
        provider: provider_from_base_remote(&base_remote),
        view_base_remote: base_remote,
        view_crypt_remote: remote_name.to_string(),
        source_path: source_path.display().to_string(),
        remote_root_path,
        remote_item_path,
        item_type: item_type.to_string(),
        display_name,
    }
}

fn persist_upload_index(record: &PendingUploadRecord) -> Result<(), String> {
    insert_upload_index_entry(record)
}

fn scoped_crypt_remote_name(base_remote: &str, crypt_suffix: &str, remote_root_path: &str) -> String {
    let normalized = remote_root_path.trim_matches('/');
    if normalized.is_empty() {
        return format!("{base_remote}{crypt_suffix}");
    }

    let slug: String = normalized
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() { "root".to_string() } else { slug };

    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    let hash = format!("{:08x}", (hasher.finish() & 0xffff_ffff) as u32);
    format!("{base_remote}{crypt_suffix}-{slug}-{hash}")
}

fn explorer_cache_key(upload_id: &str, mode: &str, dir_path: &str) -> String {
    format!("{upload_id}:{mode}:{dir_path}")
}

fn replace_explorer_cache(
    upload_id: &str,
    mode: &str,
    dir_path: &str,
    entries: &[ExplorerEntry],
    listed_at: &str,
) -> Result<(), String> {
    let connection = open_app_db()?;
    let cache_key = explorer_cache_key(upload_id, mode, dir_path);
    let transaction = connection.unchecked_transaction().map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM explorer_entries WHERE upload_id = ?1 AND mode = ?2 AND dir_path = ?3",
            params![upload_id, mode, dir_path],
        )
        .map_err(|error| error.to_string())?;

    {
        let mut insert_statement = transaction
            .prepare(
                "
                INSERT INTO explorer_entries (
                    cache_key, upload_id, mode, dir_path, entry_path, name, display_name,
                    is_dir, size, mod_time, mime_type, listed_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ",
            )
            .map_err(|error| error.to_string())?;

        if entries.is_empty() {
            insert_statement
                .execute(params![
                    cache_key,
                    upload_id,
                    mode,
                    dir_path,
                    "",
                    "",
                    "",
                    0_i64,
                    0_i64,
                    Option::<String>::None,
                    Option::<String>::None,
                    listed_at,
                ])
                .map_err(|error| error.to_string())?;
        } else {
            for entry in entries {
                insert_statement
                    .execute(params![
                        cache_key,
                        upload_id,
                        mode,
                        dir_path,
                        entry.path,
                        entry.name,
                        entry.display_name,
                        if entry.is_dir { 1_i64 } else { 0_i64 },
                        entry.size as i64,
                        entry.mod_time,
                        entry.mime_type,
                        listed_at,
                    ])
                    .map_err(|error| error.to_string())?;
            }
        }
    }

    transaction.commit().map_err(|error| error.to_string())
}

fn explorer_cache_exists(upload_id: &str, mode: &str, dir_path: &str) -> Result<bool, String> {
    let connection = open_app_db()?;
    connection
        .query_row(
            "
            SELECT EXISTS(
                SELECT 1 FROM explorer_entries
                WHERE upload_id = ?1 AND mode = ?2 AND dir_path = ?3
            )
            ",
            params![upload_id, mode, dir_path],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(|error| error.to_string())
}

fn query_explorer_entries_page(
    upload_id: &str,
    mode: &str,
    dir_path: &str,
    query: &str,
    offset: u64,
    limit: u64,
) -> Result<(Vec<ExplorerEntry>, u64, Option<String>), String> {
    let connection = open_app_db()?;
    let listed_at = connection
        .query_row(
            "
            SELECT listed_at
            FROM explorer_entries
            WHERE upload_id = ?1 AND mode = ?2 AND dir_path = ?3
            ORDER BY listed_at DESC
            LIMIT 1
            ",
            params![upload_id, mode, dir_path],
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
                FROM explorer_entries
                WHERE upload_id = ?1 AND mode = ?2 AND dir_path = ?3 AND entry_path <> ''
                  AND (name LIKE ?4 OR display_name LIKE ?4)
                ",
                params![upload_id, mode, dir_path, like_query],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())? as u64
    } else {
        connection
            .query_row(
                "
                SELECT COUNT(*)
                FROM explorer_entries
                WHERE upload_id = ?1 AND mode = ?2 AND dir_path = ?3 AND entry_path <> ''
                ",
                params![upload_id, mode, dir_path],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())? as u64
    };

    let mut statement = if filter_active {
        connection
            .prepare(
                "
                SELECT name, display_name, entry_path, is_dir, size, mod_time, mime_type
                FROM explorer_entries
                WHERE upload_id = ?1 AND mode = ?2 AND dir_path = ?3 AND entry_path <> ''
                  AND (name LIKE ?4 OR display_name LIKE ?4)
                ORDER BY is_dir DESC, LOWER(name) ASC
                LIMIT ?5 OFFSET ?6
                ",
            )
            .map_err(|error| error.to_string())?
    } else {
        connection
            .prepare(
                "
                SELECT name, display_name, entry_path, is_dir, size, mod_time, mime_type
                FROM explorer_entries
                WHERE upload_id = ?1 AND mode = ?2 AND dir_path = ?3 AND entry_path <> ''
                ORDER BY is_dir DESC, LOWER(name) ASC
                LIMIT ?4 OFFSET ?5
                ",
            )
            .map_err(|error| error.to_string())?
    };

    let entries = if filter_active {
        statement
            .query_map(
                params![upload_id, mode, dir_path, like_query, limit as i64, offset as i64],
                |row| {
                    Ok(ExplorerEntry {
                        name: row.get("name")?,
                        display_name: row.get("display_name")?,
                        path: row.get("entry_path")?,
                        is_dir: row.get::<_, i64>("is_dir")? != 0,
                        size: row.get::<_, i64>("size")? as u64,
                        mod_time: row.get("mod_time")?,
                        mime_type: row.get("mime_type")?,
                    })
                },
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    } else {
        statement
            .query_map(
                params![upload_id, mode, dir_path, limit as i64, offset as i64],
                |row| {
                    Ok(ExplorerEntry {
                        name: row.get("name")?,
                        display_name: row.get("display_name")?,
                        path: row.get("entry_path")?,
                        is_dir: row.get::<_, i64>("is_dir")? != 0,
                        size: row.get::<_, i64>("size")? as u64,
                        mod_time: row.get("mod_time")?,
                        mime_type: row.get("mime_type")?,
                    })
                },
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };

    Ok((entries, total_count, listed_at))
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
                job_id, kind, status, execute_id, provider, upload_id,
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
                record.upload_id,
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
            SELECT job_id, kind, status, execute_id, provider, upload_id, source_ref, target_ref,
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
                    kind: row.get("kind")?,
                    execute_id: row.get("execute_id")?,
                    display_name: row.get("display_name")?,
                    source_ref: row.get("source_ref")?,
                    target_ref: row.get("target_ref")?,
                    provider: row.get("provider")?,
                    upload_id: row.get("upload_id")?,
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
                    pending_upload: None,
                })
            },
        )
        .map_err(|error| format!("unknown job `{job_id}`: {error}"))
}

fn parse_serving_rc_addr(line: &str) -> Option<String> {
    let marker = "Serving remote control on ";
    let start = line.find(marker)? + marker.len();
    let url = line[start..].trim();
    let url = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    let addr = url.trim_end_matches('/').trim();
    if addr.is_empty() {
        None
    } else {
        Some(addr.to_string())
    }
}

fn pipe_sidecar_logs<T>(reader: T)
where
    T: Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => {
                    if let Some(rc_addr) = parse_serving_rc_addr(&line) {
                        let _ = set_current_rc_addr(rc_addr);
                    }
                    eprintln!("{line}");
                }
                Err(error) => {
                    eprintln!("failed to read rclone output: {error}");
                    break;
                }
            }
        }
    });
}

fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(default)
}

fn app_config() -> &'static AppConfig {
    APP_CONFIG.get_or_init(|| {
        ensure_dotenv_loaded();

        let state_dir = std::env::var("APP_STATE_DIR")
            .ok()
            .filter(|v| !v.is_empty()) // ← 追加：空文字ならNoneにする
            .map(PathBuf::from)
            .unwrap_or_else(|| sample_root().join(".state"));
        let config_path = std::env::var("APP_RCLONE_CONFIG_PATH")
            .ok()
            .filter(|v| !v.is_empty()) // ← 追加：空文字ならNoneにする
            .map(PathBuf::from)
            .unwrap_or_else(|| state_dir.join("rclone").join("rclone.conf"));

        AppConfig {
            rclone_binary: std::env::var("RCLONE_SIDECAR_NAME")
                .unwrap_or_else(|_| "rclone".to_string()),
            rc_addr: std::env::var("RCLONE_RC_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:5572".to_string()),
            rc_user: std::env::var("RCLONE_RC_USER").unwrap_or_else(|_| "local-user".to_string()),
            rc_pass: std::env::var("RCLONE_RC_PASS").unwrap_or_else(|_| "change-me".to_string()),
            config_path,
            state_dir,
            use_keychain: env_flag("APP_USE_KEYCHAIN", true),
            default_mode: std::env::var("RCLONE_DEFAULT_MODE")
                .unwrap_or_else(|_| "copy".to_string()),
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
            default_crypt_remote_suffix: std::env::var("DEFAULT_CRYPT_REMOTE_SUFFIX")
                .unwrap_or_else(|_| "-crypt".to_string()),
        }
    })
}

fn google_drive_oauth_config() -> Result<GoogleDriveOAuthConfig, String> {
    let config = app_config();
    let client_id = config
        .google_drive_client_id
        .clone()
        .ok_or_else(|| "GOOGLE_DRIVE_CLIENT_ID is not set".to_string())?;
    let client_secret = config
        .google_drive_client_secret
        .clone()
        .ok_or_else(|| "GOOGLE_DRIVE_CLIENT_SECRET is not set".to_string())?;

    Ok(GoogleDriveOAuthConfig {
        client_id,
        client_secret,
        callback_port: config.google_drive_callback_port,
        scopes: config.google_drive_scopes.clone(),
    })
}

fn ensure_state_paths() -> Result<(), String> {
    let config = app_config();

    // state_dirの作成
    fs::create_dir_all(&config.state_dir)
        .map_err(|e| format!("Failed to create state dir ({:?}): {}", config.state_dir, e))?;

    // configファイルの親ディレクトリの作成
    if let Some(parent) = config.config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config parent ({:?}): {}", parent, e))?;
    }

    // configファイル自体の作成
    if !config.config_path.exists() {
        fs::write(&config.config_path, b"").map_err(|e| {
            format!(
                "Failed to create config file ({:?}): {}",
                config.config_path, e
            )
        })?;
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
    connection
        .execute_batch(
            "
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS upload_index (
                upload_id TEXT PRIMARY KEY,
                uploaded_at TEXT NOT NULL,
                provider TEXT NOT NULL,
                view_base_remote TEXT NOT NULL,
                view_crypt_remote TEXT NOT NULL,
                source_path TEXT NOT NULL,
                remote_root_path TEXT NOT NULL,
                remote_item_path TEXT NOT NULL,
                item_type TEXT NOT NULL,
                display_name TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS explorer_entries (
                cache_key TEXT NOT NULL,
                upload_id TEXT NOT NULL,
                mode TEXT NOT NULL,
                dir_path TEXT NOT NULL,
                entry_path TEXT NOT NULL,
                name TEXT NOT NULL,
                display_name TEXT NOT NULL,
                is_dir INTEGER NOT NULL,
                size INTEGER NOT NULL,
                mod_time TEXT,
                mime_type TEXT,
                listed_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_explorer_entries_dir
                ON explorer_entries (upload_id, mode, dir_path);
            CREATE INDEX IF NOT EXISTS idx_explorer_entries_dir_name
                ON explorer_entries (upload_id, mode, dir_path, name);
            CREATE TABLE IF NOT EXISTS transfer_jobs (
                job_id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                execute_id TEXT NOT NULL,
                provider TEXT,
                upload_id TEXT,
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
        .pragma_update(None, "user_version", 1_i64)
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

fn upload_index_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UploadIndexEntry> {
    Ok(UploadIndexEntry {
        upload_id: row.get("upload_id")?,
        uploaded_at: row.get("uploaded_at")?,
        provider: row.get("provider")?,
        view_base_remote: row.get("view_base_remote")?,
        view_crypt_remote: row.get("view_crypt_remote")?,
        source_path: row.get("source_path")?,
        remote_root_path: row.get("remote_root_path")?,
        remote_item_path: row.get("remote_item_path")?,
        item_type: row.get("item_type")?,
        display_name: row.get("display_name")?,
    })
}

fn read_upload_index_entries() -> Result<Vec<UploadIndexEntry>, String> {
    let connection = open_app_db()?;
    let mut statement = connection
        .prepare(
            "
            SELECT upload_id, uploaded_at, provider, view_base_remote, view_crypt_remote,
                   source_path, remote_root_path, remote_item_path, item_type, display_name
            FROM upload_index
            ORDER BY uploaded_at DESC
            ",
        )
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], upload_index_entry_from_row)
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn insert_upload_index_entry(record: &PendingUploadRecord) -> Result<(), String> {
    let connection = open_app_db()?;
    connection
        .execute(
            "
            INSERT OR REPLACE INTO upload_index (
                upload_id, uploaded_at, provider, view_base_remote, view_crypt_remote,
                source_path, remote_root_path, remote_item_path, item_type, display_name
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                record.upload_id,
                upload_timestamp()?,
                record.provider,
                record.view_base_remote,
                record.view_crypt_remote,
                record.source_path,
                record.remote_root_path,
                record.remote_item_path,
                record.item_type,
                record.display_name,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn find_upload_index_entry_by_id(upload_id: &str) -> Result<UploadIndexEntry, String> {
    let connection = open_app_db()?;
    connection
        .query_row(
            "
            SELECT upload_id, uploaded_at, provider, view_base_remote, view_crypt_remote,
                   source_path, remote_root_path, remote_item_path, item_type, display_name
            FROM upload_index
            WHERE upload_id = ?1
            ",
            [upload_id],
            upload_index_entry_from_row,
        )
        .map_err(|error| format!("unknown upload index entry `{upload_id}`: {error}"))
}

fn normalize_relative_remote_path(path: Option<&str>) -> Result<String, String> {
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

fn join_remote_path(base: &str, relative: &str) -> String {
    let base = base.trim_matches('/');
    let relative = relative.trim_matches('/');

    match (base.is_empty(), relative.is_empty()) {
        (true, true) => String::new(),
        (false, true) => base.to_string(),
        (true, false) => relative.to_string(),
        (false, false) => format!("{base}/{relative}"),
    }
}

fn remote_spec(remote_name: &str, path: &str) -> String {
    if path.trim().is_empty() {
        format!("{remote_name}:")
    } else {
        format!("{remote_name}:{}", path.trim_matches('/'))
    }
}

fn explorer_remote_name(entry: &UploadIndexEntry) -> String {
    entry.view_crypt_remote.clone()
}

fn ensure_upload_remote_ready(entry: &UploadIndexEntry) -> Result<(), String> {
    if entry.view_base_remote == app_config().default_gdrive_remote {
        ensure_drive_remote_token_compatibility(&entry.view_base_remote)?;
    }
    Ok(())
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

    let ext = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());

    match ext.as_deref() {
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
        .and_then(|value| value.to_str())
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
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("download");
    let extension = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
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

fn rc_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("reqwest client")
}

fn rc_url(path: &str) -> String {
    format!(
        "http://{}/{}",
        current_rc_addr(),
        path.trim_start_matches('/')
    )
}

fn rc_post<T: for<'de> Deserialize<'de>>(path: &str, payload: Value) -> Result<T, String> {
    let config = app_config();
    let response = rc_client()
        .post(rc_url(path))
        .basic_auth(&config.rc_user, Some(&config.rc_pass))
        .json(&payload)
        .send()
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("rclone rc error {}: {}", status, body));
    }

    response.json::<T>().map_err(|error| error.to_string())
}

fn rc_post_value(path: &str, payload: Value) -> Result<Value, String> {
    rc_post(path, payload)
}

fn sidecar_running() -> bool {
    rc_post_value("core/version", json!({})).is_ok()
}

fn resolve_rclone_binary_path(binary_name: &str) -> PathBuf {
    let requested = PathBuf::from(binary_name);
    if requested.is_absolute() || requested.components().count() > 1 {
        return requested;
    }

    let candidates = {
        #[cfg(target_os = "windows")]
        {
            let mut names = vec![requested.clone()];
            if !binary_name.ends_with(".exe") {
                names.push(PathBuf::from(format!("{binary_name}.exe")));
            }
            names
        }

        #[cfg(not(target_os = "windows"))]
        {
            vec![requested.clone()]
        }
    };

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for candidate in &candidates {
                let bundled = exe_dir.join(candidate);
                if bundled.is_file() {
                    return bundled;
                }
            }
        }
    }

    requested
}

fn spawn_rclone_rcd_process(rc_addr: &str) -> Result<Child, String> {
    ensure_state_paths()?;
    let config = app_config();
    let rclone_binary = resolve_rclone_binary_path(&config.rclone_binary);

    let mut child = Command::new(rclone_binary)
        .arg("rcd")
        .arg("--config")
        .arg(&config.config_path)
        .arg("--rc-addr")
        .arg(rc_addr)
        .arg("--rc-user")
        .arg(&config.rc_user)
        .arg("--rc-pass")
        .arg(&config.rc_pass)
        .arg("--rc-no-auth=false")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to spawn rclone rcd: {error}"))?;

    if let Some(stdout) = child.stdout.take() {
        pipe_sidecar_logs(stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        pipe_sidecar_logs(stderr);
    }

    Ok(child)
}

fn ensure_sidecar_started() -> Result<(), String> {
    if sidecar_running() {
        return Ok(());
    }

    let _startup_guard = sidecar_startup_lock()
        .lock()
        .map_err(|_| "sidecar startup lock unavailable".to_string())?;

    if sidecar_running() {
        return Ok(());
    }

    {
        let mut state = sidecar_state()
            .lock()
            .map_err(|_| "sidecar lock unavailable".to_string())?;

        if let Some(mut child) = state.child.take() {
            match child.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                Err(error) => return Err(error.to_string()),
            }
        }

        set_current_rc_addr("127.0.0.1:0".to_string())?;
        state.child = Some(spawn_rclone_rcd_process("127.0.0.1:0")?);
    }

    for _ in 0..60 {
        if sidecar_running() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }

    cleanup_sidecar();
    Err("rclone RC API did not become ready".to_string())
}

fn keyring_key(remote_name: &str) -> String {
    format!("crypt:{remote_name}")
}

fn load_crypt_password(remote_name: &str) -> Result<Option<String>, String> {
    if !app_config().use_keychain {
        return Ok(std::env::var("APP_CRYPT_PASSWORD").ok());
    }

    let entry = keyring::Entry::new("byok-encrypted-r2-drop", &keyring_key(remote_name))
        .map_err(|error| error.to_string())?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(std::env::var("APP_CRYPT_PASSWORD").ok()),
        Err(error) => Err(error.to_string()),
    }
}

fn store_crypt_password(remote_name: &str, password: &str) -> Result<(), String> {
    if !app_config().use_keychain {
        return Ok(());
    }

    let entry = keyring::Entry::new("byok-encrypted-r2-drop", &keyring_key(remote_name))
        .map_err(|error| error.to_string())?;
    entry
        .set_password(password)
        .map_err(|error| error.to_string())
}

fn run_rclone_command(args: &[String]) -> Result<String, String> {
    let output = run_rclone_command_bytes(args)?;
    Ok(String::from_utf8_lossy(&output).trim().to_string())
}

fn run_rclone_command_bytes(args: &[String]) -> Result<Vec<u8>, String> {
    ensure_state_paths()?;
    let config = app_config();
    let rclone_binary = resolve_rclone_binary_path(&config.rclone_binary);
    let output = Command::new(rclone_binary)
        .arg("--config")
        .arg(&config.config_path)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(if stderr.is_empty() { stdout } else { stderr });
    }

    Ok(output.stdout)
}

fn stat_explorer_item(entry: &UploadIndexEntry, relative_path: &str) -> Result<RcloneLsJsonItem, String> {
    ensure_upload_remote_ready(entry)?;
    let remote_path = if relative_path.is_empty() {
        entry.remote_item_path.clone()
    } else {
        join_remote_path(&entry.remote_item_path, relative_path)
    };

    let output = run_rclone_command(&[
        "lsjson".to_string(),
        "--stat".to_string(),
        remote_spec(&explorer_remote_name(entry), &remote_path),
    ])?;

    serde_json::from_str::<RcloneLsJsonItem>(&output)
        .map_err(|error| format!("Failed to parse lsjson stat output: {error}"))
}

fn list_explorer_directory(
    entry: &UploadIndexEntry,
    relative_path: &str,
    encrypted_view: bool,
) -> Result<Vec<ExplorerEntry>, String> {
    ensure_upload_remote_ready(entry)?;
    let remote_path = if relative_path.is_empty() {
        entry.remote_item_path.clone()
    } else {
        join_remote_path(&entry.remote_item_path, relative_path)
    };

    let mut args = vec!["lsjson".to_string()];
    if encrypted_view {
        args.push("--encrypted".to_string());
    }
    args.push(remote_spec(&explorer_remote_name(entry), &remote_path));

    let output = run_rclone_command(&args)?;
    let mut items = serde_json::from_str::<Vec<RcloneLsJsonItem>>(&output)
        .map_err(|error| format!("Failed to parse lsjson output: {error}"))?;

    items.sort_by(|left, right| {
        right.is_dir.cmp(&left.is_dir).then_with(|| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        })
    });

    Ok(items
        .into_iter()
        .map(|item| {
            let child_path = if relative_path.is_empty() {
                item.path.clone()
            } else {
                join_remote_path(relative_path, &item.path)
            };

            ExplorerEntry {
                name: item.name.clone(),
                display_name: if encrypted_view {
                    item.encrypted.unwrap_or(item.name)
                } else {
                    item.name
                },
                path: child_path,
                is_dir: item.is_dir,
                size: item.size,
                mod_time: item.mod_time,
                mime_type: item.mime_type,
            }
        })
        .collect())
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
    upload_id: Option<String>,
    pending_upload: Option<PendingUploadRecord>,
) -> Result<JobRecord, String> {
    Ok(JobRecord {
        job_id,
        kind: kind.to_string(),
        execute_id,
        display_name: display_name.clone(),
        source_ref,
        target_ref,
        provider,
        upload_id,
        phase: "running".to_string(),
        progress: initial_job_progress(&display_name),
        error: None,
        result: None,
        started_at: Some(upload_timestamp()?),
        finished_at: None,
        pending_upload,
    })
}

fn parse_human_size(text: &str) -> Option<u64> {
    let cleaned = text.trim().replace(',', "");
    let mut parts = cleaned.split_whitespace();
    let value = parts.next()?.parse::<f64>().ok()?;
    let unit = parts.next().unwrap_or("B").to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "b" | "byte" | "bytes" => 1_f64,
        "kib" | "kb" | "kbytes" | "kilobytes" => 1024_f64,
        "mib" | "mb" | "mbytes" | "megabytes" => 1024_f64.powi(2),
        "gib" | "gb" | "gbytes" | "gigabytes" => 1024_f64.powi(3),
        "tib" | "tb" | "tbytes" | "terabytes" => 1024_f64.powi(4),
        _ => return None,
    };
    Some((value * multiplier).round() as u64)
}

fn parse_human_speed(text: &str) -> Option<u64> {
    parse_human_size(text.trim().trim_end_matches("/s"))
}

fn parse_eta_duration(text: &str) -> Option<u64> {
    let value = text.trim().trim_start_matches("ETA").trim();
    if value.is_empty() || value == "-" {
        return None;
    }

    let mut total = 0_u64;
    let mut current = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
            continue;
        }
        if current.is_empty() {
            continue;
        }
        let number = current.parse::<u64>().ok()?;
        current.clear();
        total += match ch {
            'd' => number * 86_400,
            'h' => number * 3_600,
            'm' => number * 60,
            's' => number,
            _ => return None,
        };
    }
    if !current.is_empty() {
        total += current.parse::<u64>().ok()?;
    }
    Some(total)
}

fn parse_transfer_stats_line(line: &str) -> Option<(u64, Option<u64>, Option<u64>, Option<u64>)> {
    let rest = line.trim().strip_prefix("Transferred:")?.trim();
    let mut parts = rest.split(',').map(str::trim);
    let amount_text = parts.next()?;
    if !amount_text.contains("B") {
        return None;
    }
    let (done_text, total_text) = amount_text.split_once('/')?;
    let bytes_done = parse_human_size(done_text)?;
    let bytes_total = parse_human_size(total_text);
    let _percent = parts.next();
    let speed = parts.next().and_then(parse_human_speed);
    let eta = parts.next().and_then(parse_eta_duration);
    Some((bytes_done, bytes_total, speed, eta))
}

fn update_job_progress_from_line(job_id: &str, line: &str) {
    if let Some((bytes_done, bytes_total, speed, eta)) = parse_transfer_stats_line(line) {
        let _ = update_job_record(job_id, |record| {
            record.progress.bytes_done = bytes_done;
            if bytes_total.is_some() {
                record.progress.bytes_total = bytes_total;
            }
            record.progress.speed = speed;
            record.progress.eta = eta;
        });
    }
}

fn spawn_output_reader<T>(reader: T, job_id: String) -> thread::JoinHandle<()>
where
    T: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffered = BufReader::new(reader);
        let mut chunk = [0_u8; 1024];
        let mut current = Vec::new();

        loop {
            match buffered.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => {
                    for byte in &chunk[..count] {
                        if *byte == b'\n' || *byte == b'\r' {
                            if !current.is_empty() {
                                update_job_progress_from_line(
                                    &job_id,
                                    String::from_utf8_lossy(&current).as_ref(),
                                );
                                current.clear();
                            }
                        } else {
                            current.push(*byte);
                        }
                    }
                }
                Err(_) => break,
            }
        }

        if !current.is_empty() {
            update_job_progress_from_line(&job_id, String::from_utf8_lossy(&current).as_ref());
        }
    })
}

fn run_streaming_rclone_job(job_id: &str, transfer_args: Vec<String>) -> Result<(), String> {
    ensure_state_paths()?;
    let config = app_config();
    let rclone_binary = resolve_rclone_binary_path(&config.rclone_binary);

    let mut child = Command::new(rclone_binary)
        .arg("--config")
        .arg(&config.config_path)
        .arg("--stats")
        .arg("1s")
        .arg("--stats-one-line")
        .arg("-P")
        .args(&transfer_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;

    let stdout_handle = child
        .stdout
        .take()
        .map(|stdout| spawn_output_reader(stdout, job_id.to_string()));
    let stderr_handle = child
        .stderr
        .take()
        .map(|stderr| spawn_output_reader(stderr, job_id.to_string()));

    let status = child.wait().map_err(|error| error.to_string())?;
    if let Some(handle) = stdout_handle {
        let _ = handle.join();
    }
    if let Some(handle) = stderr_handle {
        let _ = handle.join();
    }

    if status.success() {
        Ok(())
    } else {
        Err(format!("rclone exited with status {}", status))
    }
}

fn obscure_password(password: &str) -> Result<String, String> {
    run_rclone_command(&["obscure".to_string(), password.to_string()])
}

fn write_rclone_section(section_name: &str, entries: &[(&str, String)]) -> Result<(), String> {
    ensure_state_paths()?;
    let config_path = &app_config().config_path;

    let existing = if config_path.exists() {
        fs::read_to_string(config_path)
            .map_err(|error| format!("Failed to read config ({:?}): {}", config_path, error))?
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

    lines.push(format!("[{}]", section_name));
    for (key, value) in entries {
        lines.push(format!("{} = {}", key, value));
    }
    lines.push(String::new());

    fs::write(config_path, lines.join("\n"))
        .map_err(|error| format!("Failed to update config ({:?}): {}", config_path, error))
}

fn read_rclone_section(section_name: &str) -> Result<HashMap<String, String>, String> {
    ensure_state_paths()?;
    let config_path = &app_config().config_path;
    let existing = fs::read_to_string(config_path)
        .map_err(|error| format!("Failed to read config ({:?}): {}", config_path, error))?;

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
    let existing = fs::read_to_string(config_path)
        .map_err(|error| format!("Failed to read config ({:?}): {}", config_path, error))?;

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

    fs::write(config_path, lines.join("\n"))
        .map_err(|error| format!("Failed to update config ({:?}): {}", config_path, error))
}

fn has_rclone_section(section_name: &str) -> Result<bool, String> {
    ensure_state_paths()?;
    let config_path = &app_config().config_path;
    let existing = fs::read_to_string(config_path)
        .map_err(|error| format!("Failed to read config ({:?}): {}", config_path, error))?;
    let header = format!("[{section_name}]");
    Ok(existing.lines().any(|line| line.trim() == header))
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

    // favicon等によるノイズを無視するため、最大10回までリクエストを待ち受けるループ
    for _ in 0..10 {
        let request = server
            .recv_timeout(Duration::from_secs(180))
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Timed out waiting for Google OAuth callback".to_string())?;

        let callback_url = format!("http://127.0.0.1:{}{}", callback_port, request.url());
        let parsed = Url::parse(&callback_url).map_err(|error| error.to_string())?;
        let query: HashMap<_, _> = parsed.query_pairs().into_owned().collect();

        // 1. もしURLに "code" が含まれていない場合（favicon.ico 等）は、無視して次を待つ
        if !query.contains_key("code") {
            // ブラウザ側には適当なレスポンス（404など）を返して接続を切る
            let response = Response::empty(404);
            let _ = request.respond(response);
            continue;
        }

        // 2. 本命のリクエストが来た場合の処理
        let response = Response::from_string(
            "Google Drive authorization received. You can return to the app.",
        );
        let _ = request.respond(response);

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
        // ↓ここを変更！
        .map_err(|error| format!("Network request failed: {}", error))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("google token exchange failed {}: {}", status, body));
    }

    response.json::<Value>().map_err(|error| error.to_string())
}

fn parse_token_expiry(token: &Value) -> Result<Option<String>, String> {
    if let Some(expiry) = token.get("expiry").and_then(Value::as_str) {
        return Ok(Some(expiry.to_string()));
    }

    let expires_in = token
        .get("expires_in")
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|n| i64::try_from(n).ok()))
                .or_else(|| value.as_str().and_then(|s| s.parse::<i64>().ok()))
        });

    Ok(expires_in.map(|seconds| {
        (OffsetDateTime::now_utc() + TimeDuration::seconds(seconds.max(0)))
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
        (OffsetDateTime::now_utc() - TimeDuration::seconds(60))
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

    let token = serde_json::from_str::<Value>(token_value)
        .map_err(|error| format!("Failed to parse Google token for `{remote_name}`: {error}"))?;

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
            }
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
            }
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
            }
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
            }
        }
    };

    let token_json = match normalize_google_drive_token(&token, false) {
        Ok(value) => value,
        Err(error) => {
            return ConnectProviderResponse {
                ok: false,
                provider: "drive".to_string(),
                status: "failed".to_string(),
                next_action: error.to_string(),
                config_path,
            }
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
            next_action: format!("Remote `{}` configured in {}", remote_name, config_path),
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

fn runtime_config() -> RuntimeConfigResponse {
    let config = app_config();
    RuntimeConfigResponse {
        config_path: config.config_path.display().to_string(),
        state_dir: config.state_dir.display().to_string(),
        rc_addr: current_rc_addr(),
        default_mode: config.default_mode.clone(),
        use_keychain: config.use_keychain,
        job_poll_interval_ms: config.job_poll_interval_ms,
        default_crypt_remote_suffix: config.default_crypt_remote_suffix.clone(),
    }
}

#[tauri::command]
fn list_upload_index() -> Result<ListUploadIndexResponse, String> {
    Ok(ListUploadIndexResponse {
        uploads: read_upload_index_entries()?,
    })
}

#[tauri::command]
fn find_upload_index_entry(
    payload: FindUploadIndexEntryRequest,
) -> Result<FindUploadIndexEntryResponse, String> {
    let connection = open_app_db()?;
    let entry = connection
        .query_row(
            "
            SELECT upload_id, uploaded_at, provider, view_base_remote, view_crypt_remote,
                   source_path, remote_root_path, remote_item_path, item_type, display_name
            FROM upload_index
            WHERE provider = ?1 AND remote_item_path = ?2
            ORDER BY uploaded_at DESC
            LIMIT 1
            ",
            params![payload.provider, payload.remote_item_path],
            upload_index_entry_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(FindUploadIndexEntryResponse { entry })
}

#[tauri::command]
fn list_explorer_entries(
    payload: ListExplorerEntriesRequest,
) -> Result<ListExplorerEntriesResponse, String> {
    let upload = find_upload_index_entry_by_id(&payload.upload_id)?;
    let current_path = normalize_relative_remote_path(payload.path.as_deref())?;
    let mode = if payload.mode == "encrypted" {
        "encrypted".to_string()
    } else {
        "decrypted".to_string()
    };
    let query = payload.query.unwrap_or_default();
    let offset = payload.offset.unwrap_or(0);
    let limit = payload.limit.unwrap_or(200).clamp(1, 500);
    let refresh = payload.refresh.unwrap_or(false);

    if upload.item_type == "file" && current_path.is_empty() {
        return Ok(ListExplorerEntriesResponse {
            upload,
            current_path,
            total_count: 0,
            next_offset: None,
            entries: Vec::new(),
            listed_at: None,
        });
    }

    if refresh || !explorer_cache_exists(&payload.upload_id, &mode, &current_path)? {
        let entries = list_explorer_directory(&upload, &current_path, mode == "encrypted")?;
        let listed_at = upload_timestamp()?;
        replace_explorer_cache(&payload.upload_id, &mode, &current_path, &entries, &listed_at)?;
    }

    let (entries, total_count, listed_at) = query_explorer_entries_page(
        &payload.upload_id,
        &mode,
        &current_path,
        &query,
        offset,
        limit,
    )?;

    Ok(ListExplorerEntriesResponse {
        upload,
        current_path,
        total_count,
        next_offset: ((offset + entries.len() as u64) < total_count)
            .then_some(offset + entries.len() as u64),
        entries,
        listed_at,
    })
}

#[tauri::command]
fn start_download_explorer_item(
    payload: ExplorerItemRequest,
) -> Result<StartJobResponse, String> {
    let upload = find_upload_index_entry_by_id(&payload.upload_id)?;
    let relative_path = normalize_relative_remote_path(payload.path.as_deref())?;
    let item = stat_explorer_item(&upload, &relative_path)?;
    let downloads_dir = downloads_dir()?;
    fs::create_dir_all(&downloads_dir)
        .map_err(|error| format!("Failed to create Downloads directory: {error}"))?;

    let target_path = unique_download_path(&downloads_dir, &item.name);
    let source_ref = remote_spec(
        &explorer_remote_name(&upload),
        &if relative_path.is_empty() {
            upload.remote_item_path.clone()
        } else {
            join_remote_path(&upload.remote_item_path, &relative_path)
        },
    );
    let target_ref = target_path.display().to_string();
    let (job_id, execute_id) = next_job_identifiers();
    let record = create_job_record(
        job_id.clone(),
        execute_id,
        "download",
        item.name.clone(),
        source_ref.clone(),
        target_ref.clone(),
        Some(upload.provider.clone()),
        Some(upload.upload_id.clone()),
        None,
    )?;
    insert_or_update_job_record(record)?;

    let job_id_for_thread = job_id.clone();
    thread::spawn(move || {
        let outcome = run_streaming_rclone_job(
            &job_id_for_thread,
            vec!["copyto".to_string(), source_ref, target_ref.clone()],
        );
        match outcome {
            Ok(()) => {
                let _ = update_job_record(&job_id_for_thread, |record| {
                    record.phase = "done".to_string();
                    record.finished_at = upload_timestamp().ok();
                    record.progress.speed = None;
                    record.progress.eta = None;
                    if record.progress.bytes_total.is_none() && record.progress.bytes_done > 0 {
                        record.progress.bytes_total = Some(record.progress.bytes_done);
                    }
                    record.result = Some(json!(DownloadExplorerItemResponse { saved_path: target_ref }));
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

    Ok(StartJobResponse { job_id })
}

#[tauri::command]
fn start_preview_explorer_item(
    payload: ExplorerItemRequest,
) -> Result<StartJobResponse, String> {
    let upload = find_upload_index_entry_by_id(&payload.upload_id)?;
    let relative_path = normalize_relative_remote_path(payload.path.as_deref())?;
    let item = stat_explorer_item(&upload, &relative_path)?;
    if item.is_dir {
        return Err("Folders cannot be previewed".to_string());
    }

    let preview_path = if relative_path.is_empty() {
        upload.remote_item_path.clone()
    } else {
        join_remote_path(&upload.remote_item_path, &relative_path)
    };
    let source_ref = remote_spec(&explorer_remote_name(&upload), &preview_path);
    let (job_id, execute_id) = next_job_identifiers();
    let record = create_job_record(
        job_id.clone(),
        execute_id,
        "preview",
        item.name.clone(),
        source_ref.clone(),
        "preview".to_string(),
        Some(upload.provider.clone()),
        Some(upload.upload_id.clone()),
        None,
    )?;
    insert_or_update_job_record(record)?;

    let job_id_for_thread = job_id.clone();
    thread::spawn(move || {
        let preview_kind = classify_preview_kind(&item.name, item.mime_type.as_deref());
        let outcome: Result<Value, String> = match preview_kind {
            "image" => {
                if item.size > 1024 * 1024 {
                    Err("Image preview is limited to files up to 1 MB".to_string())
                } else {
                    match run_rclone_command_bytes(&["cat".to_string(), source_ref.clone()]) {
                        Ok(bytes) => {
                            let mime_type =
                                guess_image_mime_type(&item.name, item.mime_type.as_deref());
                            Ok(json!(PreviewExplorerItemResponse {
                                name: item.name.clone(),
                                path: relative_path.clone(),
                                mime_type: Some(mime_type.clone()),
                                kind: "image".to_string(),
                                text: Option::<String>::None,
                                image_data_url: Some(format!(
                                    "data:{mime_type};base64,{}",
                                    base64::engine::general_purpose::STANDARD.encode(bytes)
                                )),
                                truncated: false,
                                size: item.size,
                            }))
                        }
                        Err(error) => Err(error),
                    }
                }
            }
            "text" => {
                let limit = 64 * 1024;
                match run_rclone_command_bytes(&[
                    "cat".to_string(),
                    "--count".to_string(),
                    limit.to_string(),
                    source_ref.clone(),
                ]) {
                    Ok(bytes) => Ok(json!(PreviewExplorerItemResponse {
                        name: item.name.clone(),
                        path: relative_path.clone(),
                        mime_type: item.mime_type.clone(),
                        kind: "text".to_string(),
                        text: Some(String::from_utf8_lossy(&bytes).to_string()),
                        image_data_url: Option::<String>::None,
                        truncated: item.size > limit as u64,
                        size: item.size,
                    })),
                    Err(error) => Err(error),
                }
            }
            _ => Ok(json!(PreviewExplorerItemResponse {
                name: item.name.clone(),
                path: relative_path.clone(),
                mime_type: item.mime_type.clone(),
                kind: "unsupported".to_string(),
                text: Option::<String>::None,
                image_data_url: Option::<String>::None,
                truncated: false,
                size: item.size,
            })),
        };

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
                SELECT job_id, kind, status, execute_id, provider, upload_id, source_ref, target_ref,
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
                SELECT job_id, kind, status, execute_id, provider, upload_id, source_ref, target_ref,
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
                SELECT job_id, kind, status, execute_id, provider, upload_id, source_ref, target_ref,
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
                SELECT job_id, kind, status, execute_id, provider, upload_id, source_ref, target_ref,
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
        jobs: rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?,
    })
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
fn pick_folder(app: tauri::AppHandle) -> Result<PickFolderResponse, String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |folder| {
        let path = folder.and_then(|value| value.into_path().ok());
        let _ = sender.send(path.map(|value| value.display().to_string()));
    });

    let path = receiver.recv().map_err(|error| error.to_string())?;
    Ok(PickFolderResponse { path })
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
    let r2_connected = has_rclone_section(&app_config().default_r2_remote).unwrap_or(false);

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
        "r2" => {
            let account_id = std::env::var("R2_ACCOUNT_ID").ok();
            let access_key = std::env::var("R2_ACCESS_KEY_ID").ok();
            let secret_key = std::env::var("R2_SECRET_ACCESS_KEY").ok();
            let bucket_endpoint = std::env::var("R2_ENDPOINT").ok().or_else(|| {
                account_id
                    .as_ref()
                    .map(|value| format!("https://{value}.r2.cloudflarestorage.com"))
            });

            let (access_key, secret_key, endpoint) = match (access_key, secret_key, bucket_endpoint) {
                (Some(access_key), Some(secret_key), Some(endpoint)) => (access_key, secret_key, endpoint),
                _ => {
                    return ConnectProviderResponse {
                        ok: false,
                        provider: payload.provider,
                        status: "missing_env".to_string(),
                        next_action: "Set R2_ACCOUNT_ID / R2_ACCESS_KEY_ID / R2_SECRET_ACCESS_KEY / R2_ENDPOINT in .env".to_string(),
                        config_path,
                    }
                }
            };

            let remote_name = app_config().default_r2_remote.clone();
            let entries = [
                ("type", "s3".to_string()),
                ("provider", "Cloudflare".to_string()),
                ("access_key_id", access_key),
                ("secret_access_key", secret_key),
                ("endpoint", endpoint),
                ("env_auth", "false".to_string()),
            ];

            match write_rclone_section(&remote_name, &entries) {
                Ok(_) => ConnectProviderResponse {
                    ok: true,
                    provider: payload.provider,
                    status: "configured".to_string(),
                    next_action: format!("Remote `{}` created in {}", remote_name, config_path),
                    config_path,
                },
                Err(error) => ConnectProviderResponse {
                    ok: false,
                    provider: payload.provider,
                    status: "failed".to_string(),
                    next_action: error,
                    config_path,
                },
            }
        }
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
fn create_crypt_remote(
    payload: CreateCryptRemoteRequest,
) -> Result<CreateCryptRemoteResponse, String> {
    ensure_state_paths()?;

    let remote_root_path = normalize_relative_remote_path(payload.remote_root_path.as_deref())?;
    let crypt_remote =
        scoped_crypt_remote_name(&payload.base_remote, &payload.crypt_suffix, &remote_root_path);
    let password = match payload.password.as_deref() {
        Some(value) if !value.is_empty() => {
            store_crypt_password(&crypt_remote, value)?;
            value.to_string()
        }
        _ => load_crypt_password(&crypt_remote)?.ok_or_else(|| {
            "No crypt password supplied. Provide one or set APP_CRYPT_PASSWORD.".to_string()
        })?,
    };

    let obscured = obscure_password(&password)?;
    let remote_target = remote_spec(&payload.base_remote, &remote_root_path);
    let entries = [
        ("type", "crypt".to_string()),
        ("remote", remote_target),
        ("filename_encryption", "standard".to_string()),
        ("directory_name_encryption", "true".to_string()),
        ("password", obscured),
    ];
    write_rclone_section(&crypt_remote, &entries)?;

    Ok(CreateCryptRemoteResponse {
        ok: true,
        base_remote: payload.base_remote,
        crypt_remote,
        config_path: app_config().config_path.display().to_string(),
    })
}

#[tauri::command]
fn start_upload(payload: StartUploadRequest) -> Result<StartUploadResponse, String> {
    let base_remote = infer_base_remote(&payload.remote_name);
    if base_remote == app_config().default_gdrive_remote {
        ensure_drive_remote_token_compatibility(&base_remote)?;
    }

    let source_path = PathBuf::from(&payload.source_path);
    let (job_id, execute_id) = next_job_identifiers();
    let pending_upload = build_pending_upload_record(
        &execute_id,
        &source_path,
        &payload.remote_name,
        &payload.remote_path,
    );
    let target_ref = remote_spec(&payload.remote_name, &pending_upload.remote_item_path);
    let record = create_job_record(
        job_id.clone(),
        execute_id.clone(),
        "upload",
        pending_upload.display_name.clone(),
        payload.source_path.clone(),
        target_ref.clone(),
        Some(pending_upload.provider.clone()),
        Some(pending_upload.upload_id.clone()),
        Some(pending_upload.clone()),
    )?;
    insert_or_update_job_record(record)?;

    let job_id_for_thread = job_id.clone();
    let upload_remote_name = payload.remote_name.clone();
    let upload_remote_path = pending_upload.remote_item_path.clone();
    let upload_mode = payload.mode.clone();
    thread::spawn(move || {
        let outcome = if source_path.is_file() {
            run_streaming_rclone_job(
                &job_id_for_thread,
                vec![
                    "copyto".to_string(),
                    payload.source_path.clone(),
                    remote_spec(&upload_remote_name, &upload_remote_path),
                ],
            )
        } else {
            run_streaming_rclone_job(
                &job_id_for_thread,
                vec![
                    upload_mode,
                    payload.source_path.clone(),
                    remote_spec(&upload_remote_name, &upload_remote_path),
                ],
            )
        };

        match outcome {
            Ok(()) => {
                let completed = update_job_record(&job_id_for_thread, |record| {
                    record.phase = "done".to_string();
                    record.finished_at = upload_timestamp().ok();
                    record.progress.speed = None;
                    record.progress.eta = None;
                    if record.progress.bytes_total.is_none() && record.progress.bytes_done > 0 {
                        record.progress.bytes_total = Some(record.progress.bytes_done);
                    }
                });

                if let Ok(record) = completed {
                    if let Some(pending_upload) = record.pending_upload.as_ref() {
                        let _ = persist_upload_index(pending_upload);
                    }
                }
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

    Ok(StartUploadResponse { job_id, execute_id })
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
        "list_upload_index" => match list_upload_index() {
            Ok(response) => json!(response),
            Err(error) => json!({ "ok": false, "error": error }),
        },
        "find_upload_index_entry" => {
            match serde_json::from_value::<FindUploadIndexEntryRequest>(envelope.payload) {
                Ok(payload) => match find_upload_index_entry(payload) {
                    Ok(response) => json!(response),
                    Err(error) => json!({ "ok": false, "error": error }),
                },
                Err(error) => json!({ "ok": false, "error": error.to_string() }),
            }
        }
        "list_explorer_entries" => {
            match serde_json::from_value::<ListExplorerEntriesRequest>(envelope.payload) {
                Ok(payload) => match list_explorer_entries(payload) {
                    Ok(response) => json!(response),
                    Err(error) => json!({ "ok": false, "error": error }),
                },
                Err(error) => json!({ "ok": false, "error": error.to_string() }),
            }
        }
        "start_download_explorer_item" => {
            match serde_json::from_value::<ExplorerItemRequest>(envelope.payload) {
                Ok(payload) => match start_download_explorer_item(payload) {
                    Ok(response) => json!(response),
                    Err(error) => json!({ "ok": false, "error": error }),
                },
                Err(error) => json!({ "ok": false, "error": error.to_string() }),
            }
        }
        "start_preview_explorer_item" => {
            match serde_json::from_value::<ExplorerItemRequest>(envelope.payload) {
                Ok(payload) => match start_preview_explorer_item(payload) {
                    Ok(response) => json!(response),
                    Err(error) => json!({ "ok": false, "error": error }),
                },
                Err(error) => json!({ "ok": false, "error": error.to_string() }),
            }
        }
        "list_jobs" => match serde_json::from_value::<Option<ListJobsRequest>>(envelope.payload) {
            Ok(payload) => match list_jobs(payload) {
                Ok(response) => json!(response),
                Err(error) => json!({ "ok": false, "error": error }),
            },
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        },
        "connect_provider" => {
            match serde_json::from_value::<ConnectProviderRequest>(envelope.payload) {
                Ok(payload) => json!(connect_provider(payload)),
                Err(error) => json!({ "ok": false, "error": error.to_string() }),
            }
        }
        "create_crypt_remote" => {
            match serde_json::from_value::<CreateCryptRemoteRequest>(envelope.payload) {
                Ok(payload) => match create_crypt_remote(payload) {
                    Ok(result) => json!(result),
                    Err(error) => json!({ "ok": false, "error": error }),
                },
                Err(error) => json!({ "ok": false, "error": error.to_string() }),
            }
        }
        "start_upload" => match serde_json::from_value::<StartUploadRequest>(envelope.payload) {
            Ok(payload) => match start_upload(payload) {
                Ok(result) => json!(result),
                Err(error) => json!({ "ok": false, "error": error }),
            },
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        },
        "get_job_status" => match serde_json::from_value::<GetJobStatusRequest>(envelope.payload) {
            Ok(payload) => match get_job_status(payload) {
                Ok(status) => json!(status),
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

fn cleanup_sidecar() {
    if let Ok(mut state) = sidecar_state().lock() {
        if let Some(mut child) = state.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ping,
            check_env,
            get_runtime_config,
            pick_folder,
            get_providers,
            get_provider_statuses,
            list_upload_index,
            find_upload_index_entry,
            list_explorer_entries,
            start_download_explorer_item,
            start_preview_explorer_item,
            list_jobs,
            connect_provider,
            create_crypt_remote,
            start_upload,
            get_job_status
        ])
        .plugin(tauri_plugin_dialog::init())
        .build(tauri::generate_context!())
        .expect("error while building byok-encrypted-r2-drop")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. } => {
                    cleanup_sidecar();
                }
                tauri::RunEvent::WindowEvent { label, event, .. } => {
                    if label == "main"
                        && matches!(
                            event,
                            tauri::WindowEvent::CloseRequested { .. }
                                | tauri::WindowEvent::Destroyed
                        )
                    {
                        cleanup_sidecar();
                        app_handle.exit(0);
                    }
                }
                _ => {}
            }
        });
}
