//! Integration tests against a real rclone binary using its `local` backend.
//! These verify the stat/exists/preview path the app relies on without needing
//! Google credentials. They FAIL (not skip) when rclone is missing, so the CI
//! run is meaningful; CI installs a pinned rclone.

use std::path::{Path, PathBuf};
use vault_server::config::AppConfig;
use vault_server::error::VaultError;
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

fn require_rclone() -> PathBuf {
    match rclone_binary() {
        Some(bin) => bin,
        None => panic!(
            "rclone binary not found; install rclone or set RCLONE_BINARY (CI installs a pinned release)"
        ),
    }
}

fn test_cfg(root: &Path) -> AppConfig {
    std::env::set_var(
        "VAULT_COOKIE_SECRET",
        "test-secret-that-is-long-enough-for-hmac-0123456789",
    );
    let mut cfg = vault_server::config::load();
    cfg.rclone_binary = require_rclone();
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
    let _ = require_rclone();

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

    // list() must deserialize rclone's PascalCase output (regression for the
    // rename_all fix).
    let entries = store.list("Vault").await.unwrap();
    assert!(
        entries.iter().any(|e| e.name == "vault.v1.json"),
        "list() should include vault.v1.json, got {:?}",
        entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
    );

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

#[tokio::test]
async fn stat_error_classification() {
    let _ = require_rclone();

    let root = tempfile::tempdir().unwrap();
    let cfg = test_cfg(root.path());

    // Missing path -> Ok(None), not an error.
    let store = plain_store(&cfg, "classify", root.path());
    let missing = store.stat("Vault/does-not-exist.json").await;
    match missing {
        Ok(None) => {}
        other => panic!("missing path must be Ok(None), got {:?}", other),
    }

    // Invalid config (remote not defined) -> Err (propagated, not Ok(None)).
    let bad = Rclone::for_session(&cfg, "classify").with_workdir(root.path().to_path_buf());
    let out = bad.run_with_status(&[
        "lsjson".to_string(),
        "--stat".to_string(),
        "bogus:path.txt".to_string(),
    ]);
    match out {
        Err(e) => {
            assert!(
                !e.is_not_found(),
                "config error must not be treated as not-found"
            );
            assert_ne!(e.exit_code, Some(3));
        }
        other => panic!("invalid config must be Err, got {:?}", other),
    }

    // Generic exit 1 (nonexistent remote section) -> Err, not Ok(None).
    let store = plain_store(&cfg, "classify", root.path());
    let missing_remote = store
        .rclone
        .run_with_status(&["lsjson".to_string(), "nosuchremote:".to_string()]);
    match missing_remote {
        Err(e) => {
            assert!(!e.is_not_found());
            assert_eq!(e.exit_code, Some(1));
        }
        other => panic!("generic exit 1 must be Err, got {:?}", other),
    }

    // Authentication-style failure (invalid/expired credentials) must also be
    // Err, not treated as not-found. Simulated with an s3 remote that has no
    // credentials: rclone fails with a 403/credential error (exit 1).
    let authed = Rclone::for_session(&cfg, "classify-auth").with_workdir(root.path().to_path_buf());
    let _ = authed.run_text(&[
        "config".to_string(),
        "create".to_string(),
        "s3auth".to_string(),
        "s3".to_string(),
        "provider".to_string(),
        "AWS".to_string(),
    ]);
    let broken = Rclone::for_session(&cfg, "classify-auth").with_workdir(root.path().to_path_buf());
    let out = broken.run_with_status(&[
        "lsjson".to_string(),
        "--stat".to_string(),
        "s3auth:bucket/key".to_string(),
    ]);
    match out {
        Err(e) => {
            assert!(
                !e.is_not_found(),
                "auth failure must not be treated as not-found: {}",
                e.stderr
            );
        }
        other => panic!("auth failure must be Err, got {:?}", other),
    }
}

#[tokio::test]
async fn stat_propagates_errors_through_store() {
    let _ = require_rclone();

    let root = tempfile::tempdir().unwrap();
    let cfg = test_cfg(root.path());

    // A DriveStore whose config references a remote that doesn't exist: stat()
    // must surface the rclone error, not Ok(None).
    let rclone = Rclone::for_session(&cfg, "classify-2").with_workdir(root.path().to_path_buf());
    let store = DriveStore {
        rclone,
        encrypted: false,
    };
    let result = store.stat("Vault/anything.json").await;
    match result {
        Err(VaultError::RcloneCommand(e)) => {
            assert!(!e.is_not_found());
        }
        other => panic!("expected RcloneCommand error, got {:?}", other),
    }
}

/// Concurrent config writes to the same session must not lose a section: the
/// per-session mutex serializes read-modify-write on the rclone config.
#[test]
fn concurrent_section_writes_are_serialized() {
    let _ = require_rclone();

    let root = tempfile::tempdir().unwrap();
    let cfg = test_cfg(root.path());

    let rclone = Rclone::for_session(&cfg, "lock-test");
    let rclone = std::sync::Arc::new(rclone);
    let mut handles = Vec::new();
    for i in 0..8 {
        let rc = std::sync::Arc::clone(&rclone);
        handles.push(std::thread::spawn(move || {
            rc.write_section(&format!("sec{i}"), &[("value", format!("v{i}"))])
                .unwrap();
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    // All eight sections must be present.
    for i in 0..8 {
        let config = std::fs::read_to_string(&rclone.config).unwrap();
        assert!(
            config.contains(&format!("[sec{i}]")),
            "section [sec{i}] lost in concurrent writes"
        );
    }
}
