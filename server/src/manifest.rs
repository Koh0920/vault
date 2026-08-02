use crate::error::{err, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultManifest {
    pub schema_version: u32,
    pub vault_id: String,
    pub created_at: String,
    pub key_fingerprint: String,
    pub cipher_root: String,
}

pub const MANIFEST_VERSION: u32 = 1;
pub const VAULT_ROOT_DIR: &str = "Vault";
pub const CIPHER_ROOT_DEFAULT: &str = "cipher";

pub fn vault_manifest(vault_id: String, created_at: String, key_fingerprint: String) -> VaultManifest {
    VaultManifest {
        schema_version: MANIFEST_VERSION,
        vault_id,
        created_at,
        key_fingerprint,
        cipher_root: CIPHER_ROOT_DEFAULT.to_string(),
    }
}

/// Validates a relative path used for remote storage. Rejects traversal (`..`),
/// backslashes, and rclone remote injection (`://`). Leading/trailing slashes
/// are normalized away because paths are always joined onto a fixed remote root.
pub fn validate_relative_path(path: &str) -> Result<String> {
    if path.contains(':') {
        return err("path must be relative");
    }
    let raw = path.trim().trim_matches('/');
    if raw.is_empty() {
        return Ok(String::new());
    }
    let mut components = Vec::new();
    for component in raw.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            return err("path traversal is not allowed");
        }
        if component.contains('\\') {
            return err("path traversal is not allowed");
        }
        components.push(component.to_string());
    }
    Ok(components.join("/"))
}

pub fn join_remote_path(base: &str, relative: &str) -> String {
    let base = base.trim_matches('/');
    let relative = relative.trim_matches('/');
    match (base.is_empty(), relative.is_empty()) {
        (true, true) => String::new(),
        (false, true) => base.to_string(),
        (true, false) => relative.to_string(),
        (false, false) => format!("{base}/{relative}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        assert!(validate_relative_path("../etc/passwd").is_err());
        assert!(validate_relative_path("a/../../b").is_err());
        assert!(validate_relative_path("..").is_err());
    }

    #[test]
    fn rejects_absolute() {
        assert!(validate_relative_path("drive:Vault/secret").is_err());
        assert!(validate_relative_path("https://evil.example/x").is_err());
        assert!(validate_relative_path("s3://bucket/key").is_err());
    }

    #[test]
    fn normalizes() {
        assert_eq!(validate_relative_path("a//b/./c/").unwrap(), "a/b/c");
        assert_eq!(validate_relative_path("").unwrap(), "");
        assert_eq!(validate_relative_path("/backup/sub").unwrap(), "backup/sub");
    }

    #[test]
    fn join_paths() {
        assert_eq!(join_remote_path("Vault/cipher/files", "a.txt"), "Vault/cipher/files/a.txt");
        assert_eq!(join_remote_path("Vault", ""), "Vault");
        assert_eq!(join_remote_path("", "x"), "x");
    }
}