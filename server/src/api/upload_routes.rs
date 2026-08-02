use crate::api::{session_from_cookie, ApiError};
use crate::drive::OAuthToken;
use crate::error::{Result, VaultError};
use crate::manifest::validate_relative_path;
use crate::rclone::Rclone;
use crate::vault;
use crate::AppState;
use axum::extract::{Multipart, State};
use axum::http::HeaderMap;
use axum::{routing, Json, Router};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/uploads", routing::post(upload_files))
}

fn require_token_and_key(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(OAuthToken, [u8; 32], String)> {
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

/// RAII guard that removes the temp file on drop, including success and error
/// paths. rclone `copyto` does not move the source, so even a successful copy
/// must remove the temp file to avoid filling the disk.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    async fn create(dir: &Path) -> Result<Self> {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(VaultError::Io)?;
        let path = dir.join(format!("{}-upload.tmp", uuid::Uuid::new_v4().simple()));
        Ok(TempFile { path })
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn upload_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> std::result::Result<Json<Value>, ApiError> {
    let (_token, key, session_id) = require_token_and_key(&state, &headers)?;

    // connect_crypt runs blocking rclone (obscure + config write); keep it off
    // the async runtime.
    let cfg = state.cfg.clone();
    let session_id_connect = session_id.clone();
    tokio::task::spawn_blocking(move || vault::connect_crypt(&cfg, &key, &session_id_connect))
        .await
        .map_err(|e| ApiError(VaultError::Message(format!("task join: {e}"))))??;

    let max_bytes = state.cfg.max_upload_bytes;
    let max_files = state.cfg.max_upload_files;
    let temp_dir = state.cfg.temp_dir.clone();
    let cfg = state.cfg.clone();

    let mut uploaded = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| VaultError::Message(e.to_string()))?
    {
        let Some(name) = field.file_name().map(str::to_string) else {
            continue;
        };
        let target = validate_relative_path(&name)?;
        if target.is_empty() {
            continue;
        }

        file_count += 1;
        if file_count > max_files {
            return Err(ApiError(VaultError::TooLarge(format!(
                "too many files: {file_count} > {max_files}"
            ))));
        }

        // Stream the field into a temp file with async writes, enforcing the
        // per-file / total limits and cleaning up on any error path via RAII.
        let temp = TempFile::create(&temp_dir).await?;
        {
            let mut file = tokio::fs::File::create(&temp.path)
                .await
                .map_err(VaultError::Io)?;
            let mut field_bytes: u64 = 0;
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| VaultError::Message(e.to_string()))?
            {
                field_bytes += chunk.len() as u64;
                total_bytes += chunk.len() as u64;
                if field_bytes > max_bytes || total_bytes > max_bytes {
                    return Err(ApiError(VaultError::TooLarge(format!(
                        "upload exceeds {max_bytes} bytes limit"
                    ))));
                }
                file.write_all(&chunk).await.map_err(VaultError::Io)?;
            }
            file.flush().await.map_err(VaultError::Io)?;
        }

        let dest_spec = crate::rclone::crypt_spec(&target);
        let tmp_path = temp.path.clone();
        let copy_result = {
            let rclone = Rclone::for_session(&cfg, &session_id);
            tokio::task::spawn_blocking(move || {
                rclone.run(&[
                    "copyto".to_string(),
                    tmp_path.display().to_string(),
                    dest_spec,
                ])
            })
            .await
            .map_err(|e| VaultError::Message(format!("task join: {e}")))?
        };

        match copy_result {
            Ok(_) => uploaded.push(json!({ "name": target, "ok": true })),
            Err(e) => uploaded.push(json!({ "name": target, "ok": false, "error": e.to_string() })),
        }
    }

    Ok(Json(json!({
        "ok": true,
        "uploaded": uploaded,
        "count": uploaded.len(),
    })))
}
