use crate::config::AppConfig;
use crate::error::{err, Result, VaultError};
use base64::Engine;
use chacha20poly1305::aead::OsRng;
use rand::RngCore as _;
use reqwest::Url;
use serde_json::Value as Json;
use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct OAuthToken {
    pub access_token: String,
    pub token_type: String,
    pub refresh_token: Option<String>,
    pub expiry: Option<i64>,
    pub json: String,
}

pub fn oauth_client_id(cfg: &AppConfig) -> Result<String> {
    cfg.google_client_id
        .clone()
        .ok_or_else(|| VaultError::Drive("GOOGLE_CLIENT_ID is not set".into()))
}

pub fn oauth_client_secret(cfg: &AppConfig) -> Result<String> {
    cfg.google_client_secret
        .clone()
        .ok_or_else(|| VaultError::Drive("GOOGLE_CLIENT_SECRET is not set".into()))
}

/// Builds the Google OAuth2 authorization URL. Returns (url, state, code_verifier).
pub fn build_auth_url(cfg: &AppConfig) -> Result<(String, String, String)> {
    let client_id = oauth_client_id(cfg)?;
    let mut verifier_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut verifier_bytes);
    let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(verifier_bytes);
    let code_challenge = {
        let digest = Sha256::digest(code_verifier.as_bytes());
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
    };
    let state = format!("vault-{}", uuid::Uuid::new_v4().simple());

    let url = Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", client_id.as_str()),
            ("redirect_uri", cfg.google_redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", "https://www.googleapis.com/auth/drive"),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", state.as_str()),
            ("code_challenge", code_challenge.as_str()),
            ("code_challenge_method", "S256"),
        ],
    )
    .map_err(|e| VaultError::Drive(e.to_string()))?;

    Ok((url.to_string(), state, code_verifier))
}

pub fn normalize_token(
    access_token: &str,
    token_type: &str,
    refresh_token: Option<&str>,
    expires_in: Option<i64>,
) -> Result<String> {
    let expiry = expires_in
        .map(|secs| {
            (OffsetDateTime::now_utc() + time::Duration::seconds(secs.max(0)))
                .format(&Rfc3339)
                .unwrap_or_else(|_| OffsetDateTime::now_utc().format(&Rfc3339).unwrap())
        })
        .unwrap_or_else(|| OffsetDateTime::now_utc().format(&Rfc3339).unwrap());

    let mut map = serde_json::Map::new();
    map.insert(
        "access_token".to_string(),
        Json::String(access_token.to_string()),
    );
    map.insert(
        "token_type".to_string(),
        Json::String(token_type.to_string()),
    );
    map.insert("expiry".to_string(), Json::String(expiry));
    if let Some(rt) = refresh_token {
        map.insert("refresh_token".to_string(), Json::String(rt.to_string()));
    }
    Ok(Json::Object(map).to_string())
}

/// Exchanges an authorization `code` for tokens.
pub async fn exchange_code(cfg: &AppConfig, code: &str, code_verifier: &str) -> Result<OAuthToken> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", oauth_client_id(cfg)?.as_str()),
            ("client_secret", oauth_client_secret(cfg)?.as_str()),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", cfg.google_redirect_uri.as_str()),
            ("code_verifier", code_verifier),
        ])
        .send()
        .await?;

    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return err(format!("google token exchange failed {status}: {body}"));
    }
    let token: Json = serde_json::from_str(&body)?;

    let access_token = token
        .get("access_token")
        .and_then(Json::as_str)
        .ok_or_else(|| VaultError::Drive("no access_token in response".into()))?;
    let token_type = token
        .get("token_type")
        .and_then(Json::as_str)
        .unwrap_or("Bearer");
    let refresh_token = token.get("refresh_token").and_then(Json::as_str);
    let expires_in = token.get("expires_in").and_then(Json::as_i64);

    let json = normalize_token(access_token, token_type, refresh_token, expires_in)?;
    Ok(OAuthToken {
        access_token: access_token.to_string(),
        token_type: token_type.to_string(),
        refresh_token: refresh_token.map(str::to_string),
        expiry: expires_in,
        json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn normalize_token_includes_expiry() {
        let json = normalize_token("abc", "Bearer", Some("rt"), Some(3600)).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["access_token"], "abc");
        assert_eq!(value["refresh_token"], "rt");
        assert!(value["expiry"].as_str().is_some());
    }
}
