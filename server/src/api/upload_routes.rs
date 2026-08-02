use crate::api::{ApiError, session_from_cookie};
use crate::drive::OAuthToken;
use crate::error::{Result, VaultError};
use crate::manifest::validate_relative_path;
use crate::vault;
use crate::AppState;
use axum::extract::{Multipart, State};
use axum::http::HeaderMap;
use axum::{Json, Router, routing};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/uploads", routing::post(upload_files))
}

fn require_token_and_key(state: &AppState, headers: &HeaderMap) -> Result<(OAuthToken, [u8; 32])> {
    let session = session_from_cookie(headers, &state.sessions, &state.cfg)
        .ok_or_else(|| VaultError::Message("no session".into()))?;
    let token = session
        .token
        .clone()
        .ok_or_else(|| VaultError::Message("not connected to google drive".into()))?;
    let key = session
        .master_key
        .ok_or_else(|| VaultError::Message("vault not unlocked".into()))?;
    Ok((token, key))
}

/// Streams a multipart upload to a temp file, then copies it into the crypt
/// remote. Files are never fully held in memory.
async fn upload_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> std::result::Result<Json<Value>, ApiError> {
    let (_token, key) = require_token_and_key(&state, &headers)?;
    vault::connect_crypt(&state.cfg, &key)?;

    std::fs::create_dir_all(&state.cfg.temp_dir).map_err(VaultError::Io)?;

    let mut uploaded = Vec::new();
    let mut multipart = multipart;

    while let Some(mut field) = multipart.next_field().await.map_err(|e| VaultError::Message(e.to_string()))? {
        let Some(name) = field.file_name().map(str::to_string) else {
            continue;
        };
        let target = validate_relative_path(&name)?;
        if target.is_empty() {
            continue;
        }

        let tmp_path = PathBuf::from(&state.cfg.temp_dir)
            .join(format!("{}-upload.tmp", uuid::Uuid::new_v4().simple()));

        {
            let mut file = std::fs::File::create(&tmp_path).map_err(VaultError::Io)?;
            while let Some(chunk) = field.chunk().await.map_err(|e| VaultError::Message(e.to_string()))? {
                file.write_all(&chunk).map_err(VaultError::Io)?;
            }
            file.flush().map_err(VaultError::Io)?;
        }

        let dest_spec = crate::rclone::crypt_spec(&target);
        let copy_result = crate::rclone::run_rclone(
            &state.cfg,
            &[
                "copyto".to_string(),
                tmp_path.display().to_string(),
                dest_spec,
            ],
        );
        let _ = std::fs::remove_file(&tmp_path);

        match copy_result {
            Ok(_) => uploaded.push(json!({ "name": target, "ok": true })),
            Err(e) => uploaded.push(json!({ "name": target, "ok": false, "error": e.to_string() })),
        }
    }

    Ok(Json(json!({
        "ok": true,
        "uploaded": uploaded,
    })))
}