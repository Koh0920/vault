use crate::config::AppConfig;
use crate::error::{Result, VaultError};
use crate::manifest::validate_relative_path;
use crate::rclone::{self, Rclone};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub mod_time: Option<String>,
    pub mime_type: Option<String>,
    pub encrypted: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RcloneLsJsonItem {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub is_dir: bool,
    #[serde(default, deserialize_with = "deserialize_size")]
    pub size: u64,
    #[serde(default)]
    pub mod_time: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub encrypted: Option<String>,
}

fn deserialize_size<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Json>::deserialize(deserializer)?;
    let Some(value) = value else { return Ok(0) };
    match value {
        Json::Number(n) => Ok(n.as_u64().unwrap_or(0)),
        _ => Ok(0),
    }
}

/// An object store bound to a single signed-in session. All reads/writes go
/// through the session's own rclone config so parallel sessions are isolated.
///
/// - Plain mode resolves paths directly to the drive remote (`drive:<path>`).
/// - Crypt mode resolves paths into the crypt remote root (`drive-crypt:<path>`),
///   which itself points at `drive:Vault/cipher`.
#[derive(Debug, Clone)]
pub struct DriveStore {
    pub rclone: Rclone,
    /// Whether to route through the crypt remote (encrypted view) or the drive
    /// remote (plain view). Defaults to crypt.
    pub encrypted: bool,
}

impl DriveStore {
    pub fn new(rclone: Rclone, encrypted: bool) -> Self {
        DriveStore { rclone, encrypted }
    }

    pub fn new_plain(cfg: &AppConfig, session_id: &str) -> Self {
        DriveStore {
            rclone: Rclone::for_session(cfg, session_id),
            encrypted: false,
        }
    }

    fn remote(&self, relative_path: &str) -> String {
        if self.encrypted {
            rclone::crypt_spec(relative_path)
        } else {
            rclone::drive_spec(relative_path)
        }
    }

    pub async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let path = validate_relative_path(path)?;
        self.rclone.run(&["cat".to_string(), self.remote(&path)])
    }

    /// Reads at most `limit` bytes of a remote file (`rclone cat --count`).
    /// Used to bound preview downloads so a large file can't be pulled whole.
    pub async fn get_limited(&self, path: &str, limit: u64) -> Result<Vec<u8>> {
        let path = validate_relative_path(path)?;
        self.rclone.run(&[
            "cat".to_string(),
            "--count".to_string(),
            limit.to_string(),
            self.remote(&path),
        ])
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let bytes = self.get(path).await?;
        serde_json::from_slice(&bytes).map_err(VaultError::Serde)
    }

    pub async fn put(&self, path: &str, data: &[u8]) -> Result<()> {
        let path = validate_relative_path(path)?;
        self.put_tempfile(&path, data).await
    }

    async fn put_tempfile(&self, path: &str, data: &[u8]) -> Result<()> {
        let dir = self.rclone.config_dir().join("store");
        std::fs::create_dir_all(&dir).map_err(VaultError::Io)?;
        let tmp = dir.join(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
        std::fs::write(&tmp, data).map_err(VaultError::Io)?;
        let result = self.rclone.run(&[
            "copyto".to_string(),
            tmp.display().to_string(),
            self.remote(path),
        ]);
        let _ = std::fs::remove_file(&tmp);
        result.map(|_| ())
    }

    pub async fn list(&self, path: &str) -> Result<Vec<ObjectEntry>> {
        let path = validate_relative_path(path)?;
        let mut args = vec!["lsjson".to_string()];
        if self.encrypted {
            args.push("--encrypted".to_string());
        }
        args.push(self.remote(&path));
        let output = self.rclone.run(&args)?;
        let items: Vec<RcloneLsJsonItem> = serde_json::from_slice(&output)
            .map_err(|e| VaultError::Meta(format!("parse lsjson: {e}")))?;
        let mut entries: Vec<ObjectEntry> = items
            .into_iter()
            .map(|item| {
                let child = if path.is_empty() {
                    item.path.clone()
                } else {
                    crate::manifest::join_remote_path(&path, &item.path)
                };
                ObjectEntry {
                    name: item.name.clone(),
                    path: child,
                    is_dir: item.is_dir,
                    size: item.size,
                    mod_time: item.mod_time.clone(),
                    mime_type: item.mime_type.clone(),
                    encrypted: item.encrypted.clone(),
                }
            })
            .collect();
        entries.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
        });
        Ok(entries)
    }

    pub async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.stat(path).await?.is_some())
    }

    /// Returns metadata (including size) for a single remote path without
    /// downloading it, using `rclone lsjson --stat`.
    pub async fn stat(&self, path: &str) -> Result<Option<ObjectEntry>> {
        let path = validate_relative_path(path)?;
        if path.is_empty() {
            return Ok(None);
        }
        let out = self.rclone.run(&[
            "lsjson".to_string(),
            "--stat".to_string(),
            self.remote(&path),
        ]);
        match out {
            Ok(bytes) => {
                let items: Vec<RcloneLsJsonItem> = serde_json::from_slice(&bytes)
                    .map_err(|e| VaultError::Meta(format!("parse lsjson stat: {e}")))?;
                Ok(items.into_iter().next().map(|item| ObjectEntry {
                    name: item.name.clone(),
                    path: item.path.clone(),
                    is_dir: item.is_dir,
                    size: item.size,
                    mod_time: item.mod_time.clone(),
                    mime_type: item.mime_type.clone(),
                    encrypted: item.encrypted.clone(),
                }))
            }
            Err(_) => Ok(None),
        }
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let path = validate_relative_path(path)?;
        self.rclone
            .run(&["purge".to_string(), self.remote(&path)])
            .map(|_| ())
    }

    /// Creates a remote directory (encrypted name when routed via crypt).
    pub async fn mkdir(&self, path: &str) -> Result<()> {
        let path = validate_relative_path(path)?;
        if path.is_empty() {
            return Ok(());
        }
        self.rclone
            .run(&["mkdir".to_string(), self.remote(&path)])
            .map(|_| ())
    }

    pub async fn write_json<T: Serialize>(&self, path: &str, value: &T) -> Result<()> {
        let bytes =
            serde_json::to_vec(value).map_err(|e| VaultError::Meta(format!("encode: {e}")))?;
        self.put(path, &bytes).await
    }
}
