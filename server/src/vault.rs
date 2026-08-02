use crate::config::AppConfig;
use crate::crypto;
use crate::drive::OAuthToken;
use crate::error::{err, Result, VaultError};
use crate::manifest::{vault_manifest, VaultManifest};
use crate::rclone::{self, DRIVE_REMOTE};
use crate::storage::{DriveStore, ObjectEntry};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const DIR_CIPHER: &str = "cipher";
pub const FILE_MANIFEST: &str = "Vault/vault.v1.json";
pub const FILE_ENVELOPE: &str = "Vault/key-envelope.v1.json";
pub const DIR_FILES: &str = "Vault/cipher/files";
pub const DIR_VAULT_META: &str = "Vault/cipher/.vault";

#[derive(Debug, Clone)]
pub struct UnlockedVault {
    pub manifest: VaultManifest,
    pub master_key: [u8; 32],
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStateResponse {
    pub vault_exists: bool,
    pub vault_id: Option<String>,
    pub key_fingerprint: Option<String>,
    pub unlocked: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub vault_id: String,
    pub recovery_key: String,
    pub key_fingerprint: String,
}

pub fn now_iso() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| VaultError::Meta(e.to_string()))
}

fn make_id() -> String {
    format!("{}", uuid::Uuid::new_v4().simple())
}

/// Connects Google Drive (writes the rclone remote with the OAuth token) and
/// returns the plain DriveStore used for metadata.
pub fn connect_drive(cfg: &AppConfig, token: &OAuthToken) -> Result<DriveStore> {
    let client_id = crate::drive::oauth_client_id(cfg)?;
    let client_secret = crate::drive::oauth_client_secret(cfg)?;
    rclone::write_drive_remote(cfg, &client_id, &client_secret, &token.json)?;
    Ok(DriveStore::new(cfg, false))
}

pub fn connect_crypt(cfg: &AppConfig, master_key: &[u8]) -> Result<DriveStore> {
    let (password, password2) = crypto::derive_rclone_secrets(master_key)?;
    let password = rclone::obscure_password(cfg, &password)?;
    let password2 = rclone::obscure_password(cfg, &password2)?;
    let remote_target = rclone::remote_spec(DRIVE_REMOTE, DIR_CIPHER);
    rclone::write_crypt_remote(cfg, &remote_target, &password, &password2)?;
    Ok(DriveStore::new(cfg, true))
}

pub async fn vault_status(cfg: &AppConfig, token: &OAuthToken) -> Result<VaultStateResponse> {
    let plain = connect_drive(cfg, token)?;
    let manifest_exists = plain.exists(FILE_MANIFEST).await?;
    if !manifest_exists {
        return Ok(VaultStateResponse {
            vault_exists: false,
            vault_id: None,
            key_fingerprint: None,
            unlocked: false,
        });
    }
    let manifest: VaultManifest = plain.get_json(FILE_MANIFEST).await?;
    Ok(VaultStateResponse {
        vault_exists: true,
        vault_id: Some(manifest.vault_id.clone()),
        key_fingerprint: Some(manifest.key_fingerprint.clone()),
        unlocked: false,
    })
}

/// Creates a new vault on Drive. Generates master + recovery keys, writes the
/// manifest and the encrypted key envelope. Returns the one-time recovery key.
pub async fn initialize_vault(cfg: &AppConfig, token: &OAuthToken) -> Result<InitializeResponse> {
    let plain = connect_drive(cfg, token)?;
    if plain.exists(FILE_MANIFEST).await? {
        return err("a vault already exists; reconnect to open it instead");
    }

    let master_key = crypto::generate_master_key()?;
    let recovery_key = crypto::generate_recovery_key()?;
    let recovery_key_b64 = crypto::recovery_key_b64(&recovery_key);

    let envelope = crypto::wrap_master_key(&master_key, &recovery_key)?;
    let fingerprint = crypto::key_fingerprint(&master_key);
    let manifest = vault_manifest(make_id(), now_iso()?, fingerprint.clone());

    plain.write_json(FILE_MANIFEST, &manifest).await?;
    plain.write_json(FILE_ENVELOPE, &envelope).await?;

    // Ensure cipher layout directories exist.
    for dir in [DIR_CIPHER, DIR_FILES, DIR_VAULT_META] {
        let _ = rclone::run_rclone(cfg, &["mkdir".to_string(), rclone::cipher_spec(cfg, dir)]);
    }

    // Configure crypt remote so subsequent encrypted operations work immediately.
    let (password, password2) = crypto::derive_rclone_secrets(&master_key)?;
    let password_obscured = rclone::obscure_password(cfg, &password)?;
    let password2_obscured = rclone::obscure_password(cfg, &password2)?;
    let remote_target = rclone::remote_spec(DRIVE_REMOTE, DIR_CIPHER);
    rclone::write_crypt_remote(cfg, &remote_target, &password_obscured, &password2_obscured)?;

    Ok(InitializeResponse {
        vault_id: manifest.vault_id,
        recovery_key: recovery_key_b64,
        key_fingerprint: fingerprint,
    })
}


/// Unlocks an existing vault with a recovery key. Configures the crypt remote
/// and returns the manifested vault with its master key.
pub async fn unlock_vault(
    cfg: &AppConfig,
    token: &OAuthToken,
    recovery_key: &str,
) -> Result<UnlockedVault> {
    let plain = connect_drive(cfg, token)?;
    let manifest: VaultManifest = plain.get_json(FILE_MANIFEST).await.map_err(|_| {
        VaultError::NotFound("vault manifest not found on Drive; connect an existing vault first".into())
    })?;
    let envelope: crypto::KeyEnvelope = plain.get_json(FILE_ENVELOPE).await?;

    let recovery = crypto::recovery_key_from_b64(recovery_key)?;
    let master_key = crypto::unwrap_master_key(&envelope, &recovery)?;

    let (password, password2) = crypto::derive_rclone_secrets(&master_key)?;
    let password_obscured = rclone::obscure_password(cfg, &password)?;
    let password2_obscured = rclone::obscure_password(cfg, &password2)?;
    let remote_target = rclone::remote_spec(DRIVE_REMOTE, DIR_CIPHER);
    rclone::write_crypt_remote(cfg, &remote_target, &password_obscured, &password2_obscured)?;

    Ok(UnlockedVault { manifest, master_key })
}

pub async fn list_root(cfg: &AppConfig, master_key: &[u8]) -> Result<Vec<ObjectEntry>> {
    connect_crypt(cfg, master_key)?;
    let store = DriveStore::new(cfg, true);
    store.list("").await
}

