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

/// Writes an executable fake `rclone` script that always exits 1 with the given
/// message on stderr. Used to deterministically exercise error classification
/// without depending on network or backend behavior.
fn fake_rclone_script(dir: &Path, stderr_msg: &str) -> PathBuf {
    let fake = dir.join("fake-rclone");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = format!("#!/bin/sh\necho '{stderr_msg}' >&2\nexit 1\n");
        std::fs::write(&fake, script).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(not(unix))]
    {
        let _ = stderr_msg;
        std::fs::write(&fake, "exit 1\n").unwrap();
    }
    fake
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

    // Missing path -> Ok(None), not an error. Verified against the real rclone
    // local backend (exit 3 "directory not found").
    let store = plain_store(&cfg, "classify", root.path());
    let missing = store.stat("Vault/does-not-exist.json").await;
    match missing {
        Ok(None) => {}
        other => panic!("missing path must be Ok(None), got {:?}", other),
    }

    // Every other failure class is verified deterministically with a fake
    // rclone binary, so CI never depends on network/backend behavior.
    // 1) invalid config / generic exit 1 -> Err, not Ok(None).
    let fake_exit1 = fake_rclone_script(root.path(), "didn't find section in config file");
    let mut cfg1 = test_cfg(root.path());
    cfg1.rclone_binary = fake_exit1;
    let r1 = Rclone::for_session(&cfg1, "classify").with_workdir(root.path().to_path_buf());
    let out = r1.run_with_status(
        &[
            "lsjson".to_string(),
            "--stat".to_string(),
            "bogus:path.txt".to_string(),
        ],
        "lsjson --stat",
    );
    match out {
        Err(e) => {
            assert!(!e.is_not_found(), "config error must not be not-found");
            assert_ne!(e.exit_code, Some(3));
        }
        other => panic!("invalid config must be Err, got {:?}", other),
    }

    // 2) auth-style failure -> Err, not Ok(None).
    let fake_auth = fake_rclone_script(root.path(), "auth failed: permission denied");
    let mut cfg2 = test_cfg(root.path());
    cfg2.rclone_binary = fake_auth;
    let r2 = Rclone::for_session(&cfg2, "classify-auth").with_workdir(root.path().to_path_buf());
    let out = r2.run_with_status(
        &[
            "lsjson".to_string(),
            "--stat".to_string(),
            "s3auth:bucket/key".to_string(),
        ],
        "lsjson --stat",
    );
    match out {
        Err(e) => {
            assert!(!e.is_not_found(), "auth failure must not be not-found");
        }
        other => panic!("auth failure must be Err, got {:?}", other),
    }
}

#[tokio::test]
async fn stat_propagates_errors_through_store() {
    let _ = require_rclone();

    let root = tempfile::tempdir().unwrap();
    // Fake rclone that always fails (deterministic; no network dependency).
    let mut cfg = test_cfg(root.path());
    cfg.rclone_binary = fake_rclone_script(root.path(), "boom");

    // A DriveStore whose rclone invocation fails: stat() must surface the
    // rclone error, not Ok(None).
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

/// A fake rclone that always fails on `obscure`; verifies the crypt password is
/// never leaked into error messages or API responses.
#[test]
fn obscure_failure_does_not_leak_secret() {
    let root = tempfile::tempdir().unwrap();

    // Satisfy config::load() (tests run off loopback).
    std::env::set_var(
        "VAULT_COOKIE_SECRET",
        "test-secret-that-is-long-enough-for-hmac-0123456789",
    );

    // Write an executable script that always exits 1 regardless of args.
    let fake = fake_rclone_script(root.path(), "obscure failed");

    let mut cfg = vault_server::config::load();
    cfg.rclone_binary = fake;
    cfg.state_dir = root.path().join("state");
    cfg.temp_dir = root.path().join("tmp");
    let secret = "must-not-appear-super-secret-crypt-password";
    let rclone = Rclone::for_session(&cfg, "redact-test");
    let err = rclone
        .obscure_password(secret)
        .expect_err("obscure must fail against the fake binary");

    // Neither the error Display (server log path) nor the API user message
    // may contain the password.
    let display = err.to_string();
    assert!(
        !display.contains(secret),
        "Display leaked secret: {display}"
    );

    let api_message = err.user_message();
    assert!(
        !api_message.contains(secret),
        "API message leaked secret: {api_message}"
    );
    // The safe label mentions the operation without the argument; the API
    // message must not contain raw stderr.
    assert!(
        api_message.contains("obscure"),
        "expected generic message, got {api_message}"
    );
    assert!(
        !api_message.contains("obscure failed"),
        "API message must not expose raw stderr: {api_message}"
    );

    // Also confirm a successful path would never embed it, and the RcloneError
    // operation label is redacted.
    match err {
        VaultError::RcloneCommand(e) => {
            assert_eq!(e.operation, "obscure <redacted>");
            assert!(!e.stderr.contains(secret));
        }
        other => panic!("expected RcloneCommand, got {other:?}"),
    }
}
