use crate::api::{ApiError, session_from_cookie};
use crate::drive::OAuthToken;
use crate::error::{Result, VaultError};
use crate::manifest::validate_relative_path;
use crate::storage::DriveStore;
use crate::vault;
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::{Json, Router, routing};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/files", routing::get(list_files))
        .route("/api/v1/files/preview", routing::post(preview_file))
}

#[derive(Deserialize)]
struct ListParams {
    path: Option<String>,
}

fn require_token_and_key(state: &AppState, headers: &HeaderMap) -> Result<(OAuthToken, [u8; 32], String)> {
    let session = session_from_cookie(headers, &state.sessions, &state.cfg)
        .ok_or_else(|| VaultError::Message("no session".into()))?;
    let token = session
        .token
        .clone()
        .ok_or_else(|| VaultError::Message("not connected to google drive".into()))?;
    let key = session
        .master_key
        .ok_or_else(|| VaultError::Message("vault not unlocked".into()))?;
    Ok((token, key, session.id))
}

async fn list_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<ListParams>,
) -> std::result::Result<Json<Value>, ApiError> {
    let (_token, key, session_id) = require_token_and_key(&state, &headers)?;
    let path = validate_relative_path(params.path.as_deref().unwrap_or(""))?;

    let cfg = state.cfg.clone();
    let path_for_task = path.clone();
    let entries = tokio::task::spawn_blocking(move || {
        vault::connect_crypt(&cfg, &key, &session_id)?;
        let store = DriveStore::new(crate::rclone::Rclone::for_session(&cfg, &session_id), true);
        // Blocking rclone call; run off the async runtime.
        futures::executor::block_on(store.list(&path_for_task))
    })
    .await
    .map_err(|e| ApiError(VaultError::Message(format!("task join: {e}"))))??;

    Ok(Json(json!({
        "ok": true,
        "path": path,
        "entries": entries,
    })))
}

#[derive(Deserialize)]
struct PreviewParams {
    path: String,
}

async fn preview_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<PreviewParams>,
) -> std::result::Result<Json<Value>, ApiError> {
    let (_token, key, session_id) = require_token_and_key(&state, &headers)?;
    let path = validate_relative_path(&params.path)?;

    let max_preview = state.cfg.max_preview_bytes;
    let cfg = state.cfg.clone();
    let path_for_task = path.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        vault::connect_crypt(&cfg, &key, &session_id)?;
        let store = DriveStore::new(crate::rclone::Rclone::for_session(&cfg, &session_id), true);
        futures::executor::block_on(store.get(&path_for_task))
    })
    .await
    .map_err(|e| ApiError(VaultError::Message(format!("task join: {e}"))))??;

    if bytes.len() > max_preview {
        return Err(ApiError(VaultError::TooLarge(format!(
            "file is {} bytes; preview limited to {} bytes",
            bytes.len(),
            max_preview
        ))));
    }

    let text = String::from_utf8(bytes.clone()).ok();
    let mime = mime_guess(&params.path);
    if text.is_none() && mime == "application/octet-stream" {
        return Err(ApiError(VaultError::Forbidden("binary files cannot be previewed".into())));
    }
    Ok(Json(json!({
        "ok": true,
        "path": params.path,
        "mimeType": mime,
        "text": text,
        "size": bytes.len(),
    })))
}

fn mime_guess(name: &str) -> String {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" => format!("image/{ext}"),
        "md" | "markdown" | "txt" | "json" | "js" | "ts" | "css" | "html" | "rs" | "py"
        | "toml" | "yaml" | "yml" | "csv" | "log" | "sh" => "text/plain".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}
