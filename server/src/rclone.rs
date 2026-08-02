use crate::config::AppConfig;
use crate::error::{Result, VaultError};
use crate::manifest::join_remote_path;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

pub const DRIVE_REMOTE: &str = "drive";
pub const DRIVE_CRYPT_REMOTE: &str = "drive-crypt";

pub fn config_path(cfg: &AppConfig) -> PathBuf {
    cfg.state_dir.join("rclone").join("rclone.conf")
}

pub fn ensure_config(cfg: &AppConfig) -> Result<PathBuf> {
    let path = config_path(cfg);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(VaultError::Io)?;
    }
    if !path.exists() {
        std::fs::write(&path, b"").map_err(VaultError::Io)?;
    }
    Ok(path)
}

pub fn run_rclone(cfg: &AppConfig, args: &[String]) -> Result<Vec<u8>> {
    let config = ensure_config(cfg)?;
    let output = Command::new(&cfg.rclone_binary)
        .arg("--config")
        .arg(&config)
        .args(args)
        .output()
        .map_err(|e| VaultError::Rclone(format!("failed to run rclone: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(VaultError::Rclone(detail));
    }
    Ok(output.stdout)
}

pub fn run_rclone_text(cfg: &AppConfig, args: &[String]) -> Result<String> {
    Ok(String::from_utf8_lossy(&run_rclone(cfg, args)?).trim().to_string())
}

pub fn obscure_password(cfg: &AppConfig, password: &str) -> Result<String> {
    run_rclone_text(cfg, &["obscure".to_string(), password.to_string()])
}

fn write_section(cfg: &AppConfig, section: &str, entries: &[(&str, String)]) -> Result<()> {
    let path = ensure_config(cfg)?;
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

pub fn read_section(cfg: &AppConfig, section: &str) -> Result<HashMap<String, String>> {
    let path = config_path(cfg);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let existing = std::fs::read_to_string(&path).map_err(VaultError::Io)?;
    let mut in_section = false;
    let mut entries = HashMap::new();
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let current = &trimmed[1..trimmed.len() - 1];
            if in_section && current != section {
                break;
            }
            in_section = current == section;
            continue;
        }
        if !in_section || trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            entries.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    Ok(entries)
}

pub fn has_section(cfg: &AppConfig, section: &str) -> Result<bool> {
    Ok(!read_section(cfg, section)?.is_empty())
}

pub fn remove_section(cfg: &AppConfig, section: &str) -> Result<()> {
    let path = config_path(cfg);
    if !path.exists() {
        return Ok(());
    }
    let existing = std::fs::read_to_string(&path).map_err(VaultError::Io)?;
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
    std::fs::write(&path, lines.join("\n")).map_err(VaultError::Io)
}

/// Writes/refreshes the Google Drive remote section (token included).
pub fn write_drive_remote(
    cfg: &AppConfig,
    client_id: &str,
    client_secret: &str,
    token_json: &str,
) -> Result<()> {
    write_section(
        cfg,
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

/// Writes the crypt remote. `base` is the target inside the vault cipher root, e.g.
/// `drive:Vault/cipher`. `password`/`password2` should already be obscure()'d.
pub fn write_crypt_remote(cfg: &AppConfig, remote_target: &str, password: &str, password2: &str) -> Result<()> {
    write_section(
        cfg,
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

pub fn remote_spec(remote_name: &str, path: &str) -> String {
    if path.trim().is_empty() {
        format!("{remote_name}:")
    } else {
        format!("{remote_name}:{}", path.trim_matches('/'))
    }
}

/// Computes the plain remote spec under the vault cipher root.
pub fn cipher_spec(cfg: &AppConfig, relative: &str) -> String {
    let _ = cfg;
    remote_spec(DRIVE_REMOTE, &join_remote_path("Vault/cipher", relative))
}

pub fn crypt_spec(relative: &str) -> String {
    remote_spec(DRIVE_CRYPT_REMOTE, relative)
}