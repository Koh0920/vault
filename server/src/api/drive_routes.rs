use crate::api::{ApiError, session_from_cookie};
use crate::drive;
use crate::session::Session;
use crate::AppState;
use crate::auth_key;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect};
use axum::{Json, Router, routing};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::SystemTime;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/drive/oauth/start", routing::get(oauth_start))
        .route("/api/v1/drive/oauth/callback", routing::get(oauth_callback))
        .route("/api/v1/drive/status", routing::get(drive_status))
        .route("/api/v1/drive/disconnect", routing::post(drive_disconnect))
}

#[derive(Deserialize)]
struct StartParams {
    #[serde(rename = "redirectUrl")]
    #[allow(dead_code)]
    redirect: Option<String>,
}

async fn oauth_start(
    State(state): State<AppState>,
    Query(_params): Query<StartParams>,
) -> std::result::Result<axum::response::Response, ApiError> {
    let (url, oauth_state, verifier) = drive::build_auth_url(&state.cfg)?;

    let id = auth_key();
    let session = Session {
        id: id.clone(),
        token: None,
        master_key: None,
        vault_id: None,
        code_verifier: verifier,
        state: oauth_state,
        expires_at: SystemTime::now() + std::time::Duration::from_secs(3600),
        connected: false,
    };
    state.sessions.put(session);

    let cookie = state.sessions.cookie_value(&state.cfg, &id);
    let set_cookie = format!(
        "{}={cookie}; HttpOnly; Path=/; SameSite=Lax; Max-Age=3600",
        crate::api::SESSION_COOKIE
    );

    let body = json!({ "ok": true, "url": url });
    let mut resp = axum::Json(body).into_response();
    resp.headers_mut()
        .insert("Set-Cookie", set_cookie.parse().unwrap());
    let _ = _params;
    Ok(resp)
}

#[derive(Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
    error: Option<String>,
}

async fn oauth_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
    headers: HeaderMap,
) -> axum::response::Response {
    if let Some(err) = &params.error {
        return Redirect::to(&format!("/?drive=error&reason={}", err)).into_response();
    }

    let session = match session_from_cookie(&headers, &state.sessions, &state.cfg) {
        Some(s) if s.state == params.state => s,
        Some(_) => {
            return Redirect::to("/?drive=error&reason=state-mismatch").into_response();
        }
        None => {
            return Redirect::to("/?drive=error&reason=no-session").into_response();
        }
    };

    match drive::exchange_code(&state.cfg, &params.code, &session.code_verifier).await {
        Ok(token) => {
            let _ = state.sessions.update(&session.id, |s| {
                s.token = Some(token);
                s.connected = true;
                Ok(())
            });
            Redirect::to("/?drive=connected").into_response()
        }
        Err(e) => Redirect::to(&format!("/?drive=error&reason={}", e)).into_response(),
    }
}

async fn drive_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, ApiError> {
    let connected = match session_from_cookie(&headers, &state.sessions, &state.cfg) {
        Some(s) => s.token.is_some(),
        None => false,
    };
    Ok(Json(json!({ "ok": true, "connected": connected })))
}

async fn drive_disconnect(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, ApiError> {
    if let Some(s) = session_from_cookie(&headers, &state.sessions, &state.cfg) {
        state.sessions.drop_credentials(&s.id);
        let _ = crate::rclone::remove_section(&state.cfg, crate::rclone::DRIVE_REMOTE);
        let _ = crate::rclone::remove_section(&state.cfg, crate::rclone::DRIVE_CRYPT_REMOTE);
    }
    Ok(Json(json!({ "ok": true })))
}