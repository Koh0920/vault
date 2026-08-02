use crate::error::{err, Result, VaultError};
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const MASTER_KEY_LEN: usize = 32;
pub const RECOVERY_KEY_LEN: usize = 32;
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 24;

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

/// Decrypts (recovery) key for the signing/encryption envelope using HKDF + XChaCha20.
pub fn wrap_master_key(master_key: &[u8], recovery_key: &[u8]) -> Result<KeyEnvelope> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let hk = Hkdf::<Sha256>::new(Some(&salt), recovery_key);
    let mut envelope_key = [0u8; 32];
    hk.expand(b"master-key-envelope", &mut envelope_key)
        .map_err(|e| VaultError::Crypto(format!("hkdf envelope: {e}")))?;

    let cipher = XChaCha20Poly1305::new(&envelope_key.into());
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let encrypted = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            master_key,
        )
        .map_err(|e| VaultError::Crypto(format!("encrypt master key: {e}")))?;

    Ok(KeyEnvelope {
        schema_version: 1,
        algorithm: "xchacha20-poly1305".to_string(),
        kdf: "hkdf-sha256".to_string(),
        salt: base64::engine::general_purpose::STANDARD.encode(salt),
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
        encrypted_master_key: base64::engine::general_purpose::STANDARD.encode(encrypted),
    })
}

pub fn unwrap_master_key(envelope: &KeyEnvelope, recovery_key: &[u8]) -> Result<[u8; MASTER_KEY_LEN]> {
    let salt = base64::engine::general_purpose::STANDARD
        .decode(&envelope.salt)
        .map_err(|e| VaultError::Crypto(format!("salt decode: {e}")))?;
    let nonce = base64::engine::general_purpose::STANDARD
        .decode(&envelope.nonce)
        .map_err(|e| VaultError::Crypto(format!("nonce decode: {e}")))?;
    let sealed = base64::engine::general_purpose::STANDARD
        .decode(&envelope.encrypted_master_key)
        .map_err(|e| VaultError::Crypto(format!("sealed decode: {e}")))?;

    let hk = Hkdf::<Sha256>::new(Some(&salt), recovery_key);
    let mut envelope_key = [0u8; 32];
    hk.expand(b"master-key-envelope", &mut envelope_key)
        .map_err(|e| VaultError::Crypto(format!("hkdf envelope: {e}")))?;

    let cipher = XChaCha20Poly1305::new(&envelope_key.into());
    let plain = cipher
        .decrypt(XNonce::from_slice(&nonce), sealed.as_ref())
        .map_err(|_| VaultError::Crypto("failed to decrypt master key (wrong recovery key)".into()))?;

    let mut master = [0u8; MASTER_KEY_LEN];
    if plain.len() != MASTER_KEY_LEN {
        return err("decrypted master key has unexpected length");
    }
    master.copy_from_slice(&plain);
    Ok(master)
}

pub fn key_fingerprint(master_key: &[u8]) -> String {
    use sha2::Digest;
    let digest = Sha256::digest(master_key);
    format!("sha256:{}", hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_roundtrip() {
        let master = generate_master_key().unwrap();
        let recovery = generate_recovery_key().unwrap();
        let envelope = wrap_master_key(&master, &recovery).unwrap();
        let unwrapped = unwrap_master_key(&envelope, &recovery).unwrap();
        assert_eq!(&master[..], &unwrapped[..]);
    }

    #[test]
    fn wrong_recovery_key_fails() {
        let master = generate_master_key().unwrap();
        let recovery = generate_recovery_key().unwrap();
        let wrong = generate_recovery_key().unwrap();
        let envelope = wrap_master_key(&master, &recovery).unwrap();
        assert!(unwrap_master_key(&envelope, &wrong).is_err());
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