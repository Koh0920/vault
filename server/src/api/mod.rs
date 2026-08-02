pub mod drive_routes;
pub mod explorer_routes;
pub mod upload_routes;
pub mod vault_routes;

use crate::config::AppConfig;
use crate::error::VaultError;
use crate::session::{Session, SessionStore};
use crate::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

pub struct ApiError(pub VaultError);

impl From<VaultError> for ApiError {
    fn from(e: VaultError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            VaultError::NotFound(_) => StatusCode::NOT_FOUND,
            VaultError::TooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            VaultError::Forbidden(_) => StatusCode::FORBIDDEN,
            _ => StatusCode::BAD_REQUEST,
        };
        (
            status,
            Json(json!({ "ok": false, "error": self.0.to_string() })),
        )
            .into_response()
    }
}

pub const SESSION_COOKIE: &str = "vault_session";

/// Reads a valid session id from the request cookie.
pub fn session_from_cookie(
    headers: &HeaderMap,
    store: &SessionStore,
    cfg: &AppConfig,
) -> Option<Session> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(&format!("{SESSION_COOKIE}=")) {
            if let Some(id) = store.verify(cfg, value) {
                return store.get(&id);
            }
        }
    }
    None
}

/// Returns the Set-Cookie header value for a session id.
pub fn set_cookie_header(id: &str, state: &AppState) -> String {
    let value = state.sessions.cookie_value(&state.cfg, id);
    let mut header = format!(
        "{SESSION_COOKIE}={value}; HttpOnly; Path=/; SameSite=Lax; Max-Age={}",
        state.cfg.session_max_age_secs
    );
    if state.cfg.session_cookie_secure {
        header.push_str("; Secure");
    }
    header
}

pub async fn healthz() -> Json<Value> {
    Json(json!({ "ok": true }))
}

pub async fn runtime(State(state): State<AppState>) -> Json<Value> {
    let cfg = &state.cfg;
    Json(json!({
        "ok": true,
        "listenAddr": cfg.listen_addr,
        "stateDir": cfg.state_dir.display().to_string(),
        "rcloneBinary": cfg.rclone_binary.display().to_string(),
        "googleRedirectUri": cfg.google_redirect_uri,
        "defaultCryptRemoteSuffix": cfg.default_crypt_remote_suffix,
    }))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/runtime", get(runtime))
        .merge(drive_routes::routes())
        .merge(vault_routes::routes())
        .merge(explorer_routes::routes())
        .merge(upload_routes::routes())
}
