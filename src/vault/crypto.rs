//! Cryptographic primitives for the vault: AES-256-GCM + Argon2id.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Argon2, Algorithm, Params, Version};
use rand::RngCore;
use sha2::{Digest, Sha256};

pub type MasterKey = [u8; 32];
const NONCE_LEN: usize = 12;

pub fn derive_key(passphrase: &str, salt: &[u8]) -> MasterKey {
    let mut output = [0u8; 32];
    let params = Params::new(64 * 1024, 3, 4, Some(32)).expect("valid argon2 params");
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut output)
        .expect("argon2 hash");
    output
}

pub fn encrypt(key: &MasterKey, plaintext: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let cipher = Aes256Gcm::new_from_slice(key).expect("valid key");
    let mut nonce = vec![0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ciphertext = cipher.encrypt(Nonce::from_slice(&nonce), plaintext).expect("encrypt");
    (nonce, ciphertext)
}

pub fn decrypt(key: &MasterKey, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, VaultError> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("valid key");
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| VaultError::DecryptionFailed)
}

pub fn generate_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut salt);
    salt
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = [0u8; 32];
    h.copy_from_slice(&Sha256::digest(data));
    h
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let key = derive_key("pass", &[1u8; 32]);
        let (nonce, ct) = encrypt(&key, b"hello");
        assert_eq!(decrypt(&key, &nonce, &ct).unwrap(), b"hello");
    }

    #[test]
    fn test_wrong_key_fails() {
        let k1 = derive_key("a", &[1u8; 32]);
        let k2 = derive_key("b", &[1u8; 32]);
        let (nonce, ct) = encrypt(&k1, b"data");
        assert!(decrypt(&k2, &nonce, &ct).is_err());
    }

    #[test]
    fn test_deterministic() {
        let salt = [5u8; 32];
        assert_eq!(derive_key("p", &salt), derive_key("p", &salt));
    }

    #[test]
    fn test_different_salts() {
        let s1 = generate_salt();
        let s2 = generate_salt();
        assert_ne!(s1, s2);
    }
}
