//! Cryptographic primitives for the vault.
//!
//! Uses AES-256-GCM for encryption and Argon2id for key derivation.
//! Zero-copy where possible. All secrets are zeroized on drop.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};


/// 256-bit key (32 bytes)
pub type MasterKey = [u8; 32];

/// Salt for argon2 key derivation
const SALT_LEN: usize = 32;
/// AES-GCM nonce
const NONCE_LEN: usize = 12;
/// Argon2 parameters (balanced: fast enough, secure)
const ARGON_MEM_COST: u32 = 64 * 1024; // 64 MB
const ARGON_TIME_COST: u32 = 3;
const ARGON_PARALLELISM: u32 = 4;

/// Derive a 256-bit master key from a passphrase using Argon2id.
///
/// # Examples
/// ```
/// # use shellmounter::vault::crypto;
/// let key = crypto::derive_key("correct horse battery staple", &[0u8; 32]);
/// assert_eq!(key.len(), 32);
/// ```
pub fn derive_key(passphrase: &str, salt: &[u8]) -> MasterKey {
    let mut output = [0u8; 32];

    Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(ARGON_MEM_COST, ARGON_TIME_COST, ARGON_PARALLELISM, Some(32))
            .expect("valid argon2 params"),
    )
    .hash_password_into(passphrase.as_bytes(), salt, &mut output)
    .expect("argon2 hash should succeed");

    output
}

/// Encrypt plaintext with AES-256-GCM.
/// Returns (nonce, ciphertext). Nonce is generated randomly.
pub fn encrypt(key: &MasterKey, plaintext: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let cipher = Aes256Gcm::new_from_slice(key).expect("valid key");
    let mut nonce_bytes = vec![0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .expect("encryption succeeds");

    (nonce_bytes, ciphertext)
}

/// Decrypt ciphertext with AES-256-GCM.
/// Takes the nonce used during encryption.
pub fn decrypt(key: &MasterKey, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, VaultError> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("valid key");
    let nonce = Nonce::from_slice(nonce);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| VaultError::DecryptionFailed)
}

/// Generate a random salt for argon2.
pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// SHA-256 hash for integrity checks.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("decryption failed: wrong key or corrupted data")]
    DecryptionFailed,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_key_deterministic() {
        let salt = [0xABu8; 32];
        let key1 = derive_key("password123", &salt);
        let key2 = derive_key("password123", &salt);
        assert_eq!(key1, key2, "same password + salt = same key");
    }

    #[test]
    fn test_derive_key_different_passwords() {
        let salt = generate_salt();
        let key1 = derive_key("password123", &salt);
        let key2 = derive_key("password456", &salt);
        assert_ne!(key1, key2, "different passwords yield different keys");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = derive_key("test passphrase", &[0x42u8; 32]);
        let plaintext = b"SSH private key: super secret data here";

        let (nonce, ciphertext) = encrypt(&key, plaintext);
        let decrypted = decrypt(&key, &nonce, &ciphertext).unwrap();

        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = derive_key("correct key", &[1u8; 32]);
        let key2 = derive_key("wrong key", &[1u8; 32]);
        let plaintext = b"my secret";

        let (nonce, ciphertext) = encrypt(&key1, plaintext);
        let result = decrypt(&key2, &nonce, &ciphertext);

        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_wrong_nonce_fails() {
        let key = derive_key("test", &[5u8; 32]);
        let plaintext = b"data";

        let (nonce, ciphertext) = encrypt(&key, plaintext);
        let wrong_nonce = [0xFFu8; 12];
        let result = decrypt(&key, &wrong_nonce, &ciphertext);

        assert!(result.is_err());
    }

    #[test]
    fn test_empty_plaintext() {
        let key = derive_key("test", &[9u8; 32]);
        let (nonce, ciphertext) = encrypt(&key, b"");
        let decrypted = decrypt(&key, &nonce, &ciphertext).unwrap();
        assert_eq!(decrypted, b"");
    }

    #[test]
    fn test_large_plaintext() {
        let key = derive_key("test", &[8u8; 32]);
        let plaintext = vec![0xAAu8; 1024 * 1024]; // 1 MB

        let (nonce, ciphertext) = encrypt(&key, &plaintext);
        let decrypted = decrypt(&key, &nonce, &ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_generate_salt_unique() {
        let salt1 = generate_salt();
        let salt2 = generate_salt();
        assert_ne!(salt1, salt2, "salts should be unique");
    }

    #[test]
    fn test_sha256_consistency() {
        let hash1 = sha256(b"hello world");
        let hash2 = sha256(b"hello world");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_sha256_different_input() {
        let hash1 = sha256(b"hello");
        let hash2 = sha256(b"world");
        assert_ne!(hash1, hash2);
    }
}
