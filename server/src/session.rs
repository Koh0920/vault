use crate::config::AppConfig;
use crate::drive::OAuthToken;
use crate::error::{Result, VaultError};
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

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

impl Session {
    /// Zeroes secrets so dropped sessions don't leave keys in memory.
    pub fn zeroize(&mut self) {
        if let Some(key) = &mut self.master_key {
            crate::crypto::ZeroizeExt::zeroize(key);
        }
        self.token = None;
        self.vault_id = None;
        self.connected = false;
    }
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    inner: Arc<Mutex<HashMap<String, Session>>>,
    max_count: usize,
    state_dir: std::path::PathBuf,
}

fn mac_secret(cfg: &AppConfig) -> Hmac<Sha256> {
    Hmac::<Sha256>::new_from_slice(&cfg.session_cookie_secret).expect("hmac key")
}

impl SessionStore {
    pub fn new(cfg: &AppConfig) -> Self {
        SessionStore {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max_count: cfg.session_max_count,
            state_dir: cfg.state_dir.clone(),
        }
    }

    fn is_expired(session: &Session) -> bool {
        SystemTime::now() > session.expires_at
    }

    pub fn sign(&self, cfg: &AppConfig, payload: &str) -> String {
        let mut mac = mac_secret(cfg);
        mac.update(payload.as_bytes());
        let sig =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
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

    /// Insert or replace a full session, enforcing the max session count by
    /// evicting the oldest active session when at capacity. The evicted
    /// session's on-disk rclone config is removed too.
    pub fn put(&self, session: Session) {
        let mut map = self.inner.lock().unwrap();
        let active_count = map.values().filter(|s| !Self::is_expired(s)).count();
        if active_count >= self.max_count && !map.contains_key(&session.id) {
            if let Some(oldest) = map
                .iter()
                .filter(|(_, s)| !Self::is_expired(s))
                .min_by_key(|(_, s)| s.expires_at)
                .map(|(id, _)| id.clone())
            {
                if let Some(mut evicted) = map.remove(&oldest) {
                    evicted.zeroize();
                    crate::rclone::Rclone::remove_session_dir(&self.state_dir, &oldest);
                }
            }
        }
        map.insert(session.id.clone(), session);
    }

    pub fn insert(&self, session: Session) {
        self.put(session);
    }

    pub fn get(&self, id: &str) -> Option<Session> {
        let map = self.inner.lock().unwrap();
        map.get(id).filter(|s| !Self::is_expired(s)).cloned()
    }

    pub fn update<F>(&self, id: &str, mutate: F) -> Result<Option<Session>>
    where
        F: FnOnce(&mut Session) -> Result<()>,
    {
        let mut map = self.inner.lock().unwrap();
        let session = map
            .get_mut(id)
            .filter(|s| !Self::is_expired(s))
            .ok_or_else(|| VaultError::NotFound(id.to_string()))?;
        mutate(session)?;
        Ok(Some(session.clone()))
    }

    pub fn remove(&self, id: &str) {
        let mut map = self.inner.lock().unwrap();
        if let Some(mut s) = map.remove(id) {
            s.zeroize();
        }
    }

    pub fn drop_credentials(&self, id: &str) {
        if let Some(session) = self.inner.lock().unwrap().get_mut(id) {
            session.zeroize();
        }
    }

    /// Removes expired sessions (zeroizing key material) and their on-disk
    /// rclone config dirs. Active sessions are left untouched.
    pub fn gc(&self) {
        let mut map = self.inner.lock().unwrap();
        let now = SystemTime::now();
        let expired: Vec<String> = map
            .iter()
            .filter(|(_, s)| now > s.expires_at)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &expired {
            if let Some(mut s) = map.remove(id) {
                s.zeroize();
            }
        }
        drop(map);
        for id in expired {
            crate::rclone::Rclone::remove_session_dir(&self.state_dir, &id);
        }
    }

    pub fn len(&self) -> usize {
        let map = self.inner.lock().unwrap();
        map.values().filter(|s| !Self::is_expired(s)).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn cfg() -> AppConfig {
        // Tests run off loopback; provide a strong secret to satisfy config::load.
        std::env::set_var(
            "VAULT_COOKIE_SECRET",
            "test-secret-that-is-long-enough-for-hmac-0123456789",
        );
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

    #[test]
    fn expired_session_removed() {
        let cfg = cfg();
        let store = SessionStore::new(&cfg);
        let session = Session {
            id: "exp".into(),
            token: None,
            master_key: None,
            vault_id: None,
            code_verifier: String::new(),
            state: String::new(),
            expires_at: SystemTime::now() - Duration::from_secs(10),
            connected: false,
        };
        store.put(session);
        assert!(store.get("exp").is_none());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn max_count_evicts() {
        let cfg = cfg();
        let mut store = SessionStore::new(&cfg);
        store.max_count = 2;
        for i in 0..3 {
            store.put(Session {
                id: format!("s{i}"),
                token: None,
                master_key: None,
                vault_id: None,
                code_verifier: String::new(),
                state: String::new(),
                expires_at: SystemTime::now() + Duration::from_secs(60),
                connected: false,
            });
        }
        assert!(store.len() <= 2);
    }

    #[test]
    fn gc_only_removes_expired_configs() {
        let cfg = cfg();
        let store = SessionStore::new(&cfg);

        let expired = Session {
            id: "expired".into(),
            token: None,
            master_key: None,
            vault_id: None,
            code_verifier: String::new(),
            state: String::new(),
            expires_at: SystemTime::now() - Duration::from_secs(10),
            connected: false,
        };
        let active = Session {
            id: "active".into(),
            token: None,
            master_key: None,
            vault_id: None,
            code_verifier: String::new(),
            state: String::new(),
            expires_at: SystemTime::now() + Duration::from_secs(60),
            connected: false,
        };
        store.put(expired);
        store.put(active);

        // Give the expired session an on-disk config dir.
        crate::rclone::Rclone::for_session(&cfg, "expired")
            .ensure_config()
            .unwrap();
        crate::rclone::Rclone::for_session(&cfg, "active")
            .ensure_config()
            .unwrap();

        store.gc();

        assert_eq!(store.len(), 1);
        assert!(store.get("active").is_some());
        assert!(!crate::rclone::Rclone::for_session(&cfg, "expired")
            .config
            .exists());
        assert!(crate::rclone::Rclone::for_session(&cfg, "active")
            .config
            .exists());
    }

    #[test]
    fn eviction_removes_config_dir() {
        let cfg = cfg();
        let mut store = SessionStore::new(&cfg);
        store.max_count = 2;

        for i in 0..3 {
            crate::rclone::Rclone::for_session(&cfg, &format!("s{i}"))
                .ensure_config()
                .unwrap();
            store.put(Session {
                id: format!("s{i}"),
                token: None,
                master_key: None,
                vault_id: None,
                code_verifier: String::new(),
                state: String::new(),
                expires_at: SystemTime::now() + Duration::from_secs(60),
                connected: false,
            });
        }

        assert!(store.len() <= 2);
        // The first session should have been evicted, including its config.
        assert!(!crate::rclone::Rclone::for_session(&cfg, "s0")
            .config
            .exists());
    }
}
