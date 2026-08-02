use crate::error::{err, Result, VaultError};
use base64::Engine;
use chacha20poly1305::aead::{AeadInPlace, KeyInit, OsRng};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const MASTER_KEY_LEN: usize = 32;
pub const RECOVERY_KEY_LEN: usize = 32;
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 24;
/// Minimum ciphertext length for XChaCha20-Poly1305 (master key + 16B tag).
pub const MIN_SEALED_LEN: usize = MASTER_KEY_LEN + 16;
pub const ENVELOPE_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyEnvelope {
    pub schema_version: u32,
    pub algorithm: String,
    pub kdf: String,
    pub salt: String,
    pub nonce: String,
    pub encrypted_master_key: String,
}

pub fn generate_master_key() -> Result<[u8; MASTER_KEY_LEN]> {
    let mut key = [0u8; MASTER_KEY_LEN];
    OsRng.fill_bytes(&mut key);
    Ok(key)
}

pub fn generate_recovery_key() -> Result<[u8; RECOVERY_KEY_LEN]> {
    let mut key = [0u8; RECOVERY_KEY_LEN];
    OsRng.fill_bytes(&mut key);
    Ok(key)
}

pub fn recovery_key_b64(key: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key)
}

pub fn recovery_key_from_b64(input: &str) -> Result<[u8; RECOVERY_KEY_LEN]> {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(input.trim())
        .map_err(|e| VaultError::Crypto(format!("invalid recovery key: {e}")))?;
    if decoded.len() != RECOVERY_KEY_LEN {
        return err("recovery key must be exactly 32 bytes");
    }
    let mut key = [0u8; RECOVERY_KEY_LEN];
    key.copy_from_slice(&decoded);
    Ok(key)
}

/// Derives rclone crypt `password` and `password2` from the master key via HKDF-SHA256.
pub fn derive_rclone_secrets(master_key: &[u8]) -> Result<(String, String)> {
    let salt: &[u8] = b"vault-rclone-crypt-v1";
    let hk = Hkdf::<Sha256>::new(Some(salt), master_key);
    let mut password = [0u8; 32];
    let mut password2 = [0u8; 32];
    hk.expand(b"rclone-crypt-password", &mut password)
        .map_err(|e| VaultError::Crypto(format!("hkdf password: {e}")))?;
    hk.expand(b"rclone-crypt-password2", &mut password2)
        .map_err(|e| VaultError::Crypto(format!("hkdf password2: {e}")))?;
    let password_b64 = base64::engine::general_purpose::STANDARD.encode(password);
    let password2_b64 = base64::engine::general_purpose::STANDARD.encode(password2);
    Ok((password_b64, password2_b64))
}

/// Wraps the master key under a key derived from the recovery key. `aad`
/// (the vault id) is bound into the AEAD so an envelope can't be replayed
/// against a different vault.
pub fn wrap_master_key(master_key: &[u8], recovery_key: &[u8], aad: &[u8]) -> Result<KeyEnvelope> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let hk = Hkdf::<Sha256>::new(Some(&salt), recovery_key);
    let mut envelope_key = [0u8; 32];
    hk.expand(b"master-key-envelope", &mut envelope_key)
        .map_err(|e| VaultError::Crypto(format!("hkdf envelope: {e}")))?;

    let cipher = XChaCha20Poly1305::new(&envelope_key.into());
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let mut buffer = master_key.to_vec();
    cipher
        .encrypt_in_place(XNonce::from_slice(&nonce), aad, &mut buffer)
        .map_err(|e| VaultError::Crypto(format!("encrypt master key: {e}")))?;

    envelope_key.zeroize();
    Ok(KeyEnvelope {
        schema_version: ENVELOPE_VERSION,
        algorithm: "xchacha20-poly1305".to_string(),
        kdf: "hkdf-sha256".to_string(),
        salt: base64::engine::general_purpose::STANDARD.encode(salt),
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
        encrypted_master_key: base64::engine::general_purpose::STANDARD.encode(buffer),
    })
}

pub fn unwrap_master_key(envelope: &KeyEnvelope, recovery_key: &[u8], aad: &[u8]) -> Result<[u8; MASTER_KEY_LEN]> {
    // Strict structural validation before any decode/decrypt step.
    if envelope.schema_version != ENVELOPE_VERSION {
        return err(format!("unsupported envelope version {}", envelope.schema_version));
    }
    if envelope.algorithm != "xchacha20-poly1305" {
        return err(format!("unsupported algorithm {}", envelope.algorithm));
    }
    if envelope.kdf != "hkdf-sha256" {
        return err(format!("unsupported kdf {}", envelope.kdf));
    }

    let salt = base64::engine::general_purpose::STANDARD
        .decode(&envelope.salt)
        .map_err(|e| VaultError::Crypto(format!("salt decode: {e}")))?;
    let nonce = base64::engine::general_purpose::STANDARD
        .decode(&envelope.nonce)
        .map_err(|e| VaultError::Crypto(format!("nonce decode: {e}")))?;
    let sealed = base64::engine::general_purpose::STANDARD
        .decode(&envelope.encrypted_master_key)
        .map_err(|e| VaultError::Crypto(format!("sealed decode: {e}")))?;

    if salt.len() != SALT_LEN {
        return err("envelope salt has unexpected length");
    }
    if nonce.len() != NONCE_LEN {
        return err("envelope nonce has unexpected length");
    }
    if sealed.len() < MIN_SEALED_LEN {
        return err("envelope ciphertext has unexpected length");
    }

    let hk = Hkdf::<Sha256>::new(Some(&salt), recovery_key);
    let mut envelope_key = [0u8; 32];
    hk.expand(b"master-key-envelope", &mut envelope_key)
        .map_err(|e| VaultError::Crypto(format!("hkdf envelope: {e}")))?;

    let cipher = XChaCha20Poly1305::new(&envelope_key.into());
    let mut buffer = sealed;
    cipher
        .decrypt_in_place(XNonce::from_slice(&nonce), aad, &mut buffer)
        .map_err(|_| VaultError::Crypto("failed to decrypt master key (wrong recovery key)".into()))?;

    envelope_key.zeroize();

    let mut master = [0u8; MASTER_KEY_LEN];
    if buffer.len() != MASTER_KEY_LEN {
        return err("decrypted master key has unexpected length");
    }
    master.copy_from_slice(&buffer);
    Ok(master)
}

pub fn key_fingerprint(master_key: &[u8]) -> String {
    use sha2::Digest;
    let digest = Sha256::digest(master_key);
    format!("sha256:{}", hex::encode(digest))
}

/// Zeroize secrets on scope exit. Cheap safety net for key material.
pub trait ZeroizeExt {
    fn zeroize(&mut self);
}

impl ZeroizeExt for [u8] {
    fn zeroize(&mut self) {
        for b in self.iter_mut() {
            *b = 0;
        }
    }
}

impl ZeroizeExt for [u8; 32] {
    fn zeroize(&mut self) {
        for b in self.iter_mut() {
            *b = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_roundtrip() {
        let master = generate_master_key().unwrap();
        let recovery = generate_recovery_key().unwrap();
        let envelope = wrap_master_key(&master, &recovery, b"vault-123").unwrap();
        let unwrapped = unwrap_master_key(&envelope, &recovery, b"vault-123").unwrap();
        assert_eq!(&master[..], &unwrapped[..]);
    }

    #[test]
    fn wrong_recovery_key_fails() {
        let master = generate_master_key().unwrap();
        let recovery = generate_recovery_key().unwrap();
        let wrong = generate_recovery_key().unwrap();
        let envelope = wrap_master_key(&master, &recovery, b"vault-123").unwrap();
        assert!(unwrap_master_key(&envelope, &wrong, b"vault-123").is_err());
    }

    #[test]
    fn wrong_aad_fails() {
        let master = generate_master_key().unwrap();
        let recovery = generate_recovery_key().unwrap();
        let envelope = wrap_master_key(&master, &recovery, b"vault-123").unwrap();
        assert!(unwrap_master_key(&envelope, &recovery, b"vault-456").is_err());
    }

    #[test]
    fn tampered_nonce_len_rejected() {
        let master = generate_master_key().unwrap();
        let recovery = generate_recovery_key().unwrap();
        let mut envelope = wrap_master_key(&master, &recovery, b"vault-123").unwrap();
        envelope.nonce = base64::engine::general_purpose::STANDARD.encode([0u8; 10]);
        assert!(unwrap_master_key(&envelope, &recovery, b"vault-123").is_err());
    }

    #[test]
    fn wrong_version_rejected() {
        let master = generate_master_key().unwrap();
        let recovery = generate_recovery_key().unwrap();
        let mut envelope = wrap_master_key(&master, &recovery, b"vault-123").unwrap();
        envelope.schema_version = 999;
        assert!(unwrap_master_key(&envelope, &recovery, b"vault-123").is_err());
    }

    #[test]
    fn rclone_secrets_derive_consistently() {
        let master = generate_master_key().unwrap();
        let (p1, p2) = derive_rclone_secrets(&master).unwrap();
        let (q1, q2) = derive_rclone_secrets(&master).unwrap();
        assert_eq!(p1, q1);
        assert_eq!(p2, q2);
        assert!(!p1.is_empty());
    }

    #[test]
    fn recovery_key_b64_roundtrip() {
        let recovery = generate_recovery_key().unwrap();
        let encoded = recovery_key_b64(&recovery);
        let decoded = recovery_key_from_b64(&encoded).unwrap();
        assert_eq!(&recovery[..], &decoded[..]);
    }
}
