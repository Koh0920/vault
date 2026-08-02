use crate::api::{ApiError, session_from_cookie};
use crate::drive::OAuthToken;
use crate::error::{Result, VaultError};
use crate::vault;
use crate::AppState;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::{Json, routing};
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
    let status = vault::vault_status(&state.cfg, &token).await?;
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
    let resp = vault::initialize_vault(&state.cfg, &token).await?;
    // retain the master key in the session for subsequent ops (via re-derivation)
    Ok(Json(json!({
        "ok": true,
        "vaultId": resp.vault_id,
        "recoveryKey": resp.recovery_key,
        "keyFingerprint": resp.key_fingerprint,
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
    let unlocked = vault::unlock_vault(&state.cfg, &token, &body.recovery_key).await?;
    // store derived master key + vault id in session
    let _ = state.sessions.update(&session.id, |s| {
        s.master_key = Some(unlocked.master_key);
        s.vault_id = Some(unlocked.manifest.vault_id.clone());
        Ok(())
    });
    Ok(Json(json!({
        "ok": true,
        "vaultId": unlocked.manifest.vault_id,
        "keyFingerprint": unlocked.manifest.key_fingerprint,
        "recoveryKey": body.recovery_key,
    })))
}

use axum::Router;