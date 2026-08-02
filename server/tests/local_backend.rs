//! Integration tests against a real rclone binary using its `local` backend.
//! These verify the stat/exists/preview path the app relies on without needing
//! Google credentials. Tests skip (pass) when rclone is not installed.

use std::path::{Path, PathBuf};
use vault_server::config::AppConfig;
use vault_server::rclone::Rclone;
use vault_server::storage::DriveStore;

fn rclone_binary() -> Option<PathBuf> {
    let bin = std::env::var("RCLONE_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("rclone"));
    if bin.exists() {
        return Some(bin);
    }
    // Resolve via PATH.
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in path.split(':') {
        let candidate = Path::new(dir).join(&bin);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn test_cfg(root: &Path) -> AppConfig {
    std::env::set_var(
        "VAULT_COOKIE_SECRET",
        "test-secret-that-is-long-enough-for-hmac-0123456789",
    );
    let mut cfg = vault_server::config::load();
    cfg.rclone_binary = rclone_binary().unwrap_or_else(|| PathBuf::from("rclone"));
    cfg.state_dir = root.join("state");
    cfg.temp_dir = root.join("tmp");
    cfg
}

/// Configures the session's rclone config with a `[drive]` remote backed by the
/// `local` backend, and returns a plain DriveStore bound to that session. The
/// rclone process is pinned to `root` as its working directory because the
/// `local` backend resolves paths relative to cwd (the config `root` field is
/// not honored by rclone's local backend).
fn plain_store(cfg: &AppConfig, session_id: &str, root: &Path) -> DriveStore {
    let rclone = Rclone::for_session(cfg, session_id).with_workdir(root.to_path_buf());
    let out = rclone.run_text(&[
        "config".to_string(),
        "create".to_string(),
        "drive".to_string(),
        "local".to_string(),
    ]);
    assert!(out.is_ok(), "rclone config create failed: {:?}", out);
    DriveStore {
        rclone,
        encrypted: false,
    }
}

#[tokio::test]
async fn exists_stat_preview_local_backend() {
    let Some(_) = rclone_binary() else {
        eprintln!("rclone not found; skipping integration test");
        return;
    };

    let root = tempfile::tempdir().unwrap();
    let cfg = test_cfg(root.path());
    let session_id = "itest-session";

    // Seed the remote root with a manifest-like file.
    let data = r#"{"schema_version":1,"vault_id":"itest"}"#;
    let store = plain_store(&cfg, session_id, root.path());
    store
        .put("Vault/vault.v1.json", data.as_bytes())
        .await
        .unwrap();

    // exists() must report the file (regression: previously used a broken stat).
    assert!(store.exists("Vault/vault.v1.json").await.unwrap());
    assert!(!store.exists("Vault/missing.json").await.unwrap());

    // stat() must return metadata including size.
    let stat = store
        .stat("Vault/vault.v1.json")
        .await
        .unwrap()
        .expect("stat hit");
    assert!(!stat.is_dir);
    assert_eq!(stat.size as usize, data.len());

    // preview-like get must round-trip the bytes.
    let bytes = store.get("Vault/vault.v1.json").await.unwrap();
    assert_eq!(bytes, data.as_bytes());

    // Directory stat returns is_dir.
    store.mkdir("Vault/subdir").await.unwrap();
    let dir_stat = store
        .stat("Vault/subdir")
        .await
        .unwrap()
        .expect("dir stat hit");
    assert!(dir_stat.is_dir);
}
