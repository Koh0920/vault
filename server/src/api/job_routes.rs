use crate::api::{ApiError, session_from_cookie};
use crate::error::{Result, VaultError};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::{Json, Router, routing};
use serde_json::{json, Value};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/jobs", routing::get(list_jobs))
        .route("/api/v1/jobs/:id", routing::get(job_status))
        .route("/api/v1/jobs/:id/cancel", routing::post(cancel_job))
}

fn require_session(state: &AppState, headers: &HeaderMap) -> Result<crate::session::Session> {
    session_from_cookie(headers, &state.sessions, &state.cfg)
        .ok_or_else(|| VaultError::Message("no session".into()))
}

async fn list_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, ApiError> {
    let session = require_session(&state, &headers)?;
    let jobs = state.jobs.list(&session.id);
    Ok(Json(json!({ "ok": true, "jobs": jobs })))
}

async fn job_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, ApiError> {
    let session = require_session(&state, &headers)?;
    let job = state.jobs.get(&id, &session.id).ok_or_else(|| {
        ApiError(VaultError::NotFound(format!("job {id}")))
    })?;
    Ok(Json(json!(job)))
}

async fn cancel_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, ApiError> {
    let session = require_session(&state, &headers)?;
    state.jobs.cancel(&id, &session.id)?;
    Ok(Json(json!({ "ok": true, "jobId": id })))
}
