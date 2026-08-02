use crate::config::AppConfig;
use crate::error::{Result, VaultError};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DRIVE_REMOTE: &str = "drive";
pub const DRIVE_CRYPT_REMOTE: &str = "drive-crypt";

/// Structured error from a failed rclone invocation. Holds a safe, human-chosen
/// operation label (NEVER raw arguments, which may contain secrets), the process
/// exit code, and stderr so callers can distinguish "not found" from genuine
/// failures (network, auth, config, etc.).
#[derive(Debug, Clone)]
pub struct RcloneError {
    /// Safe display name for the failing operation, e.g. "obscure <redacted>",
    /// "lsjson --stat", "copyto". Never includes raw command arguments.
    pub operation: String,
    pub exit_code: Option<i32>,
    pub stderr: String,
}

impl std::fmt::Display for RcloneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = self
            .exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "?".to_string());
        write!(
            f,
            "rclone {} exited {}: {}",
            self.operation, code, self.stderr
        )
    }
}

impl std::error::Error for RcloneError {}

impl RcloneError {
    /// True when rclone reports the target does not exist (exit code 3 with a
    /// "not found" notice). All other failures return false so callers can
    /// propagate them instead of treating them as "missing".
    pub fn is_not_found(&self) -> bool {
        if self.exit_code != Some(3) {
            return false;
        }
        let lower = self.stderr.to_ascii_lowercase();
        lower.contains("not found") || lower.contains("does not exist")
    }
}

/// A handle to an rclone configuration bound to a single session. Every session
/// owns its own config file (`state/rclone/<session_id>/rclone.conf`) so parallel
/// sessions can never overwrite each other's OAuth token or crypt password.
///
/// Within one session, config writes are read-modify-write on the same file, so
/// concurrent requests (multiple tabs / parallel API calls) could lose a
/// section. `lock` serializes all rclone work per session config path.
#[derive(Clone)]
pub struct Rclone {
    pub binary: PathBuf,
    pub config: PathBuf,
    /// Optional working directory for spawned rclone processes. Used by tests
    /// to isolate the local backend (which resolves paths relative to cwd).
    pub workdir: Option<PathBuf>,
    /// Per-config mutex shared by every handle with the same config path.
    lock: std::sync::Arc<std::sync::Mutex<()>>,
}

impl std::fmt::Debug for Rclone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rclone")
            .field("binary", &self.binary)
            .field("config", &self.config)
            .field("workdir", &self.workdir)
            .finish()
    }
}

/// Global registry mapping config paths to their per-session lock. Entries are
/// held weakly: once every `Rclone` handle for a session is dropped, the entry
/// can no longer upgrade and is pruned, so the map cannot grow without bound
/// as short-lived sessions come and go.
fn lock_for(config: &Path) -> std::sync::Arc<std::sync::Mutex<()>> {
    use std::sync::{Arc, Mutex, OnceLock, Weak};
    static LOCKS: OnceLock<Mutex<std::collections::HashMap<PathBuf, Weak<Mutex<()>>>>> =
        OnceLock::new();
    let map = LOCKS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());

    // Opportunistically prune entries whose last strong handle is gone.
    map.retain(|_, weak| weak.upgrade().is_some());

    let key = config.to_path_buf();
    match map.get(&key).and_then(Weak::upgrade) {
        Some(arc) => arc,
        None => {
            let arc = Arc::new(Mutex::new(()));
            map.insert(key, Arc::downgrade(&arc));
            arc
        }
    }
}

impl Rclone {
    pub fn for_session(cfg: &AppConfig, session_id: &str) -> Self {
        Rclone::for_session_path(&cfg.state_dir, &cfg.rclone_binary, session_id)
    }

    pub fn for_session_path(state_dir: &Path, binary: &Path, session_id: &str) -> Self {
        let config = state_dir
            .join("rclone")
            .join(session_id)
            .join("rclone.conf");
        let lock = lock_for(&config);
        Rclone {
            binary: binary.to_path_buf(),
            config,
            workdir: None,
            lock,
        }
    }

    /// Sets the working directory for spawned rclone processes (test support).
    pub fn with_workdir(mut self, dir: PathBuf) -> Self {
        self.workdir = Some(dir);
        self
    }

    pub fn config_dir(&self) -> &Path {
        self.config.parent().unwrap_or(Path::new("."))
    }

    /// Removes the on-disk config directory for a session given a state dir.
    /// Acquires the same per-config lock used by run/write so a running rclone
    /// operation for that session isn't racing a GC/eviction deletion.
    pub fn remove_session_dir(state_dir: &Path, session_id: &str) {
        let config = state_dir
            .join("rclone")
            .join(session_id)
            .join("rclone.conf");
        let lock = lock_for(&config);
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(dir) = config.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    pub fn ensure_config(&self) -> Result<PathBuf> {
        if let Some(parent) = self.config.parent() {
            std::fs::create_dir_all(parent).map_err(VaultError::Io)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }
        if !self.config.exists() {
            std::fs::write(&self.config, b"").map_err(VaultError::Io)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.config, std::fs::Permissions::from_mode(0o600));
        }
        Ok(self.config.clone())
    }

    /// Removes the entire session config directory (token + crypt secrets).
    pub fn remove_all(&self) {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let _ = std::fs::remove_dir_all(self.config_dir());
    }

    pub fn run(&self, args: &[String]) -> Result<Vec<u8>> {
        // Safe default label: derive from args but redact anything after the
        // first argument of sensitive commands (obscure takes a secret).
        let operation = describe_operation(args);
        self.run_with_status(args, &operation)
            .map_err(VaultError::RcloneCommand)
    }

    /// Runs rclone and returns a structured error on failure (exit code,
    /// stderr, safe operation label) so callers can classify failures (e.g.
    /// not-found vs transient/network/auth) without leaking secrets. The
    /// caller supplies the `operation` label, which must never contain raw
    /// arguments. Serialized per session config to avoid concurrent rclone
    /// processes clobbering the same config file.
    pub fn run_with_status(
        &self,
        args: &[String],
        operation: &str,
    ) -> std::result::Result<Vec<u8>, RcloneError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let config = self.ensure_config().map_err(|e| RcloneError {
            operation: operation.to_string(),
            exit_code: None,
            stderr: format!("failed to ensure config: {e}"),
        })?;
        let mut cmd = Command::new(&self.binary);
        cmd.arg("--config").arg(&config).args(args);
        if let Some(dir) = &self.workdir {
            cmd.current_dir(dir);
        }
        let output = cmd.output().map_err(|e| RcloneError {
            operation: operation.to_string(),
            exit_code: None,
            stderr: format!("failed to run rclone: {e}"),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if stderr.is_empty() { stdout } else { stderr };
            return Err(RcloneError {
                operation: operation.to_string(),
                exit_code: output.status.code(),
                stderr: detail,
            });
        }
        Ok(output.stdout)
    }

    pub fn run_text(&self, args: &[String]) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.run(args)?).trim().to_string())
    }

    /// Obscures a password with `rclone obscure`. The error label is redacted
    /// so a failure never leaks the password into logs or API responses.
    pub fn obscure_password(&self, password: &str) -> Result<String> {
        self.run_with_status(
            &["obscure".to_string(), password.to_string()],
            "obscure <redacted>",
        )
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_string())
        .map_err(VaultError::RcloneCommand)
    }

    pub fn write_drive_remote(
        &self,
        client_id: &str,
        client_secret: &str,
        token_json: &str,
    ) -> Result<()> {
        self.write_section(
            DRIVE_REMOTE,
            &[
                ("type", "drive".to_string()),
                ("scope", "drive".to_string()),
                ("token", token_json.to_string()),
                ("client_id", client_id.to_string()),
                ("client_secret", client_secret.to_string()),
            ],
        )
    }

    /// Writes the crypt remote. `base` is the target inside the vault, e.g.
    /// `drive:Vault/cipher`. `password`/`password2` should already be obscure()'d.
    pub fn write_crypt_remote(
        &self,
        remote_target: &str,
        password: &str,
        password2: &str,
    ) -> Result<()> {
        self.write_section(
            DRIVE_CRYPT_REMOTE,
            &[
                ("type", "crypt".to_string()),
                ("remote", remote_target.to_string()),
                ("filename_encryption", "standard".to_string()),
                ("directory_name_encryption", "true".to_string()),
                ("password", password.to_string()),
                ("password2", password2.to_string()),
            ],
        )
    }

    pub fn write_section(&self, section: &str, entries: &[(&str, String)]) -> Result<()> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.ensure_config()?;
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let mut lines = Vec::new();
        let mut skipping = false;
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                let current = &trimmed[1..trimmed.len() - 1];
                skipping = current == section;
                if skipping {
                    continue;
                }
            }
            if !skipping {
                lines.push(line.to_string());
            }
        }
        while lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("[{section}]"));
        for (key, value) in entries {
            lines.push(format!("{key} = {value}"));
        }
        lines.push(String::new());
        std::fs::write(&path, lines.join("\n")).map_err(VaultError::Io)
    }

    pub fn remove_section(&self, section: &str) -> Result<()> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        if !self.config.exists() {
            return Ok(());
        }
        let existing = std::fs::read_to_string(&self.config).map_err(VaultError::Io)?;
        let mut lines = Vec::new();
        let mut skipping = false;
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                let current = &trimmed[1..trimmed.len() - 1];
                skipping = current == section;
                continue;
            }
            if !skipping {
                lines.push(line.to_string());
            }
        }
        std::fs::write(&self.config, lines.join("\n")).map_err(VaultError::Io)
    }
}

/// The plain remote spec under the vault root, e.g. `drive:Vault/vault.v1.json`.
/// Paths passed here are full relative paths from the drive root (no prefix added).
pub fn remote_spec(remote_name: &str, path: &str) -> String {
    if path.trim().is_empty() {
        format!("{remote_name}:")
    } else {
        format!("{remote_name}:{}", path.trim_matches('/'))
    }
}

/// Builds a safe, secret-free label for an rclone invocation. Only the command
/// name is shown; arguments that may hold secrets (e.g. `obscure`'s password)
/// are redacted.
fn describe_operation(args: &[String]) -> String {
    let Some(cmd) = args.first() else {
        return "rclone".to_string();
    };
    match cmd.as_str() {
        "obscure" => "obscure <redacted>".to_string(),
        other => other.to_string(),
    }
}

/// Spec for a path inside the crypt remote root, e.g. `drive-crypt:files/x.txt`.
pub fn crypt_spec(relative: &str) -> String {
    remote_spec(DRIVE_CRYPT_REMOTE, relative)
}

/// Unused helper retained for completeness; drive paths are resolved directly.
pub fn drive_spec(path: &str) -> String {
    remote_spec(DRIVE_REMOTE, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_config_sets_restrictive_permissions() {
        std::env::set_var(
            "VAULT_COOKIE_SECRET",
            "test-secret-that-is-long-enough-for-hmac-0123456789",
        );
        let mut cfg = crate::config::load();
        cfg.state_dir = std::env::temp_dir().join(format!(
            "vault-rclone-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let rclone = Rclone::for_session(&cfg, "perm-test");
        rclone.ensure_config().unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir_mode = std::fs::metadata(rclone.config_dir())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            let file_mode = std::fs::metadata(&rclone.config)
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(dir_mode, 0o700, "config dir must be 0700");
            assert_eq!(file_mode, 0o600, "config file must be 0600");
        }

        rclone.remove_all();
    }
}
