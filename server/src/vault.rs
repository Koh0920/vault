use crate::config::AppConfig;
use crate::crypto;
use crate::drive::OAuthToken;
use crate::error::{err, Result, VaultError};
use crate::manifest::{vault_manifest, VaultManifest};
use crate::rclone::{self, DRIVE_REMOTE, Rclone};
use crate::storage::{DriveStore, ObjectEntry};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub const FILE_MANIFEST: &str = "Vault/vault.v1.json";
pub const FILE_ENVELOPE: &str = "Vault/key-envelope.v1.json";
/// Base path inside Drive that the crypt remote points at.
pub const CIPHER_ROOT: &str = "Vault/cipher";
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

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub vault_id: String,
    pub recovery_key: String,
    pub key_fingerprint: String,
}

/// Result of vault creation, including the master key so the caller can keep it
/// in the session without it being serialized into the HTTP response.
#[derive(Debug, Clone)]
pub struct CreatedVault {
    pub resp: InitializeResponse,
    pub master_key: [u8; 32],
}

pub fn now_iso() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| VaultError::Meta(e.to_string()))
}

fn make_id() -> String {
    format!("{}", uuid::Uuid::new_v4().simple())
}

/// Returns the session-scoped rclone handle.
fn session_rclone(cfg: &AppConfig, session_id: &str) -> Rclone {
    Rclone::for_session(cfg, session_id)
}

/// Connects Google Drive for this session: writes the session-local rclone
/// remote with the OAuth token and returns the plain DriveStore.
pub fn connect_drive(cfg: &AppConfig, token: &OAuthToken, session_id: &str) -> Result<DriveStore> {
    let client_id = crate::drive::oauth_client_id(cfg)?;
    let client_secret = crate::drive::oauth_client_secret(cfg)?;
    let rclone = session_rclone(cfg, session_id);
    rclone.write_drive_remote(&client_id, &client_secret, &token.json)?;
    Ok(DriveStore::new(rclone, false))
}

/// Writes the session-local crypt remote derived from the master key and returns
/// the encrypted DriveStore. No plain remote is written here.
pub fn connect_crypt(cfg: &AppConfig, master_key: &[u8], session_id: &str) -> Result<DriveStore> {
    let (password, password2) = crypto::derive_rclone_secrets(master_key)?;
    let rclone = session_rclone(cfg, session_id);
    let password = rclone.obscure_password(&password)?;
    let password2 = rclone.obscure_password(&password2)?;
    let remote_target = rclone::remote_spec(DRIVE_REMOTE, CIPHER_ROOT);
    rclone.write_crypt_remote(&remote_target, &password, &password2)?;
    Ok(DriveStore::new(rclone, true))
}

pub async fn vault_status(
    cfg: &AppConfig,
    token: &OAuthToken,
    session_id: &str,
    unlocked: bool,
) -> Result<VaultStateResponse> {
    let plain = connect_drive(cfg, token, session_id)?;
    let manifest_exists = plain.exists(FILE_MANIFEST).await?;
    if !manifest_exists {
        return Ok(VaultStateResponse {
            vault_exists: false,
            vault_id: None,
            key_fingerprint: None,
            unlocked,
        });
    }
    let manifest: VaultManifest = plain.get_json(FILE_MANIFEST).await?;
    Ok(VaultStateResponse {
        vault_exists: true,
        vault_id: Some(manifest.vault_id.clone()),
        key_fingerprint: Some(manifest.key_fingerprint.clone()),
        unlocked,
    })
}

/// Creates a new vault on Drive for this session. Generates master + recovery
/// keys, writes the manifest and the encrypted key envelope, and returns the
/// one-time recovery key plus the master key for session retention.
pub async fn initialize_vault(
    cfg: &AppConfig,
    token: &OAuthToken,
    session_id: &str,
) -> Result<CreatedVault> {
    let plain = connect_drive(cfg, token, session_id)?;
    if plain.exists(FILE_MANIFEST).await? {
        return err("a vault already exists; reconnect to open it instead");
    }

    let master_key = crypto::generate_master_key()?;
    let recovery_key = crypto::generate_recovery_key()?;
    let recovery_key_b64 = crypto::recovery_key_b64(&recovery_key);

    let vault_id = make_id();
    let envelope = crypto::wrap_master_key(&master_key, &recovery_key, vault_id.as_bytes())?;
    let fingerprint = crypto::key_fingerprint(&master_key);
    let manifest = vault_manifest(vault_id.clone(), now_iso()?, fingerprint.clone());

    plain.write_json(FILE_MANIFEST, &manifest).await?;
    plain.write_json(FILE_ENVELOPE, &envelope).await?;

    // Ensure plain layout directories exist directly under the drive root.
    let rclone = session_rclone(cfg, session_id);
    for dir in ["Vault", CIPHER_ROOT, DIR_FILES, DIR_VAULT_META] {
        let _ = rclone.run(&["mkdir".to_string(), rclone::remote_spec(DRIVE_REMOTE, dir)]);
    }

    // Configure the session-local crypt remote so encrypted ops work immediately.
    connect_crypt(cfg, &master_key, session_id)?;

    Ok(CreatedVault {
        resp: InitializeResponse {
            vault_id,
            recovery_key: recovery_key_b64,
            key_fingerprint: fingerprint,
        },
        master_key,
    })
}

/// Unlocks an existing vault for this session with a recovery key. Verifies the
/// envelope and fingerprint, configures the session-local crypt remote, and
/// returns the manifested vault with its master key.
pub async fn unlock_vault(
    cfg: &AppConfig,
    token: &OAuthToken,
    recovery_key: &str,
    session_id: &str,
) -> Result<UnlockedVault> {
    let plain = connect_drive(cfg, token, session_id)?;
    let manifest: VaultManifest = plain.get_json(FILE_MANIFEST).await.map_err(|_| {
        VaultError::NotFound("vault manifest not found on Drive; connect an existing vault first".into())
    })?;
    let envelope: crypto::KeyEnvelope = plain.get_json(FILE_ENVELOPE).await?;

    let recovery = crypto::recovery_key_from_b64(recovery_key)?;
    let master_key = crypto::unwrap_master_key(&envelope, &recovery, manifest.vault_id.as_bytes())?;

    // Binding check: the envelope's fingerprint must match the manifest.
    if crypto::key_fingerprint(&master_key) != manifest.key_fingerprint {
        return err("key fingerprint mismatch: vault integrity check failed");
    }

    connect_crypt(cfg, &master_key, session_id)?;

    Ok(UnlockedVault { manifest, master_key })
}

pub async fn list_root(cfg: &AppConfig, master_key: &[u8], session_id: &str) -> Result<Vec<ObjectEntry>> {
    connect_crypt(cfg, master_key, session_id)?;
    let store = DriveStore::new(session_rclone(cfg, session_id), true);
    store.list("").await
}
