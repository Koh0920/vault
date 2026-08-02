use crate::config::AppConfig;
use crate::drive::OAuthToken;
use crate::error::{Result, VaultError};
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub token: Option<OAuthToken>,
    /// Holds the master key of the connected vault while the session is unlocked.
    pub master_key: Option<[u8; 32]>,
    pub vault_id: Option<String>,
    pub code_verifier: String,
    pub state: String,
    pub expires_at: SystemTime,
    pub connected: bool,
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    inner: Arc<Mutex<HashMap<String, Session>>>,
    #[allow(dead_code)]
    max_age: Duration,
}

fn mac_secret(cfg: &AppConfig) -> Hmac<Sha256> {
    Hmac::<Sha256>::new_from_slice(&cfg.session_cookie_secret).expect("hmac key")
}

impl SessionStore {
    pub fn new(cfg: &AppConfig) -> Self {
        let _ = cfg;
        SessionStore {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max_age: Duration::from_secs(60 * 60),
        }
    }

    fn is_expired(session: &Session) -> bool {
        SystemTime::now() > session.expires_at
    }

    pub fn sign(&self, cfg: &AppConfig, payload: &str) -> String {
        let mut mac = mac_secret(cfg);
        mac.update(payload.as_bytes());
        let sig = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!("{payload}.{sig}")
    }

    pub fn verify(&self, cfg: &AppConfig, cookie: &str) -> Option<String> {
        let (payload, sig) = cookie.rsplit_once('.')?;
        let mut mac = mac_secret(cfg);
        mac.update(payload.as_bytes());
        let sig_decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(sig)
            .ok()?;
        mac.verify_slice(&sig_decoded).ok()?;
        Some(payload.to_string())
    }

    pub fn cookie_value(&self, cfg: &AppConfig, id: &str) -> String {
        self.sign(cfg, id)
    }

    /// Insert or replace a full session.
    pub fn put(&self, session: Session) {
        self.inner.lock().unwrap().insert(session.id.clone(), session);
    }

    pub fn insert(&self, session: Session) {
        self.put(session);
    }

    pub fn get(&self, id: &str) -> Option<Session> {
        let map = self.inner.lock().unwrap();
        map.get(id)
            .filter(|s| !Self::is_expired(s))
            .cloned()
    }

    pub fn update<F>(&self, id: &str, mutate: F) -> Result<Option<Session>>
    where
        F: FnOnce(&mut Session) -> Result<()>,
    {
        let mut map = self.inner.lock().unwrap();
        let session = map
            .get_mut(id)
            .ok_or_else(|| VaultError::NotFound(id.to_string()))?;
        mutate(session)?;
        Ok(Some(session.clone()))
    }

    pub fn remove(&self, id: &str) {
        self.inner.lock().unwrap().remove(id);
    }

    pub fn drop_credentials(&self, id: &str) {
        if let Some(session) = self.inner.lock().unwrap().get_mut(id) {
            session.token = None;
            session.master_key = None;
            session.vault_id = None;
            session.connected = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AppConfig {
        crate::config::load()
    }

    #[test]
    fn cookie_roundtrip() {
        let cfg = cfg();
        let store = SessionStore::new(&cfg);
        let cookie = store.cookie_value(&cfg, "sess-123");
        assert_eq!(store.verify(&cfg, &cookie).as_deref(), Some("sess-123"));
        assert!(store.verify(&cfg, "tampered").is_none());
    }

    #[test]
    fn session_crud() {
        let cfg = cfg();
        let store = SessionStore::new(&cfg);
        let session = Session {
            id: "s1".into(),
            token: None,
            master_key: None,
            vault_id: None,
            code_verifier: String::new(),
            state: String::new(),
            expires_at: SystemTime::now() + Duration::from_secs(60),
            connected: false,
        };
        store.put(session);
        assert!(store.get("s1").is_some());
        store.drop_credentials("s1");
        assert!(store.get("s1").unwrap().token.is_none());
    }
}