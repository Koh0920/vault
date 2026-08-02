use crate::api::{session_from_cookie, ApiError};
use crate::drive::OAuthToken;
use crate::error::{Result, VaultError};
use crate::vault;
use crate::AppState;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::{routing, Json};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/vault", routing::get(vault_status))
        .route("/api/v1/vault/initialize", routing::post(vault_initialize))
        .route("/api/v1/vault/unlock", routing::post(vault_unlock))
}

fn require_token(session: &crate::session::Session) -> Result<OAuthToken> {
    session
        .token
        .clone()
        .ok_or_else(|| VaultError::Message("not connected to google drive".into()))
}

async fn vault_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, ApiError> {
    let session = session_from_cookie(&headers, &state.sessions, &state.cfg)
        .ok_or_else(|| ApiError(VaultError::Message("no session".into())))?;
    let token = require_token(&session)?;
    let unlocked = session.master_key.is_some() && session.vault_id.is_some();
    let status = vault::vault_status(&state.cfg, &token, &session.id, unlocked).await?;
    Ok(Json(json!(status)))
}

#[derive(Deserialize)]
struct InitializeBody {}

async fn vault_initialize(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_body): Json<InitializeBody>,
) -> std::result::Result<Json<Value>, ApiError> {
    let session = session_from_cookie(&headers, &state.sessions, &state.cfg)
        .ok_or_else(|| ApiError(VaultError::Message("no session".into())))?;
    let token = require_token(&session)?;
    let created = vault::initialize_vault(&state.cfg, &token, &session.id).await?;
    // Retain the master key + vault id in the session so subsequent ops and
    // status reports treat the vault as unlocked.
    let _ = state.sessions.update(&session.id, |s| {
        s.master_key = Some(created.master_key);
        s.vault_id = Some(created.resp.vault_id.clone());
        Ok(())
    });
    Ok(Json(json!({
        "ok": true,
        "vaultId": created.resp.vault_id,
        "recoveryKey": created.resp.recovery_key,
        "keyFingerprint": created.resp.key_fingerprint,
    })))
}

#[derive(Deserialize)]
struct UnlockBody {
    #[serde(rename = "recoveryKey")]
    recovery_key: String,
}

async fn vault_unlock(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UnlockBody>,
) -> std::result::Result<Json<Value>, ApiError> {
    let session = session_from_cookie(&headers, &state.sessions, &state.cfg)
        .ok_or_else(|| ApiError(VaultError::Message("no session".into())))?;
    let token = require_token(&session)?;
    let unlocked = vault::unlock_vault(&state.cfg, &token, &body.recovery_key, &session.id).await?;
    // Store the derived master key + vault id in the session.
    let _ = state.sessions.update(&session.id, |s| {
        s.master_key = Some(unlocked.master_key);
        s.vault_id = Some(unlocked.manifest.vault_id.clone());
        Ok(())
    });
    Ok(Json(json!({
        "ok": true,
        "vaultId": unlocked.manifest.vault_id,
        "keyFingerprint": unlocked.manifest.key_fingerprint,
    })))
}

use axum::Router;
