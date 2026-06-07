//! Encrypted vault for SSH keys, passwords, and certificates.
//!
//! All secrets are stored AES-256-GCM encrypted on disk.
//! The master key is derived from a passphrase via Argon2id.
//! Keys only exist decrypted in memory while the vault is unlocked.

use crate::vault::crypto::{self, MasterKey, VaultError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use zeroize::Zeroize;

/// An encrypted credential stored in the vault.
#[derive(Clone, Serialize, Deserialize)]
pub struct Secret {
    /// Unique ID
    pub id: String,
    /// Human-readable label
    pub label: String,
    /// Type of secret
    pub kind: SecretKind,
    /// Encrypted data (AES-256-GCM: nonce || ciphertext)
    pub encrypted_blob: Vec<u8>,
    /// SHA-256 of the plaintext (for integrity verification)
    pub checksum: [u8; 32],
    /// Unix timestamp when created
    pub created_at: i64,
    /// Unix timestamp when last updated
    pub updated_at: i64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum SecretKind {
    /// SSH private key (PEM format)
    SshKey,
    /// Password
    Password,
    /// SSH certificate
    Certificate,
    /// Generic secret
    Generic,
}

/// The vault state: locked or unlocked.
pub struct Vault {
    /// If unlocked, holds the derived master key
    master_key: Option<MasterKey>,
    /// Path to the encrypted store on disk
    path: String,
    /// Salt for key derivation (stored in plaintext on disk)
    salt: [u8; 32],
}

impl Vault {
    /// Create or open a vault at the given path.
    pub fn open(data_dir: &Path) -> Result<Self, VaultError> {
        let vault_dir = data_dir.join("vault");
        std::fs::create_dir_all(&vault_dir)?;

        let vault_file = vault_dir.join("secrets.db");
        let salt_file = vault_dir.join("salt");

        // Load or generate salt
        let salt = if salt_file.exists() {
            let bytes = std::fs::read(&salt_file)?;
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes[..32]);
            arr
        } else {
            let salt = crypto::generate_salt();
            std::fs::write(&salt_file, &salt)?;
            salt
        };

        Ok(Self {
            master_key: None,
            path: vault_file.to_string_lossy().to_string(),
            salt,
        })
    }

    /// Check if the vault is currently unlocked.
    pub fn is_unlocked(&self) -> bool {
        self.master_key.is_some()
    }

    /// Unlock the vault with a passphrase.
    /// Derives the master key using Argon2id + stored salt.
    pub fn unlock(&mut self, passphrase: &str) -> Result<(), VaultError> {
        // Note: passphrase is zeroized after key derivation.
        let mut pass = passphrase.to_string();
        let key = crypto::derive_key(&pass, &self.salt);
        pass.zeroize();

        // Verify by trying to decrypt at least one secret
        if Path::new(&self.path).exists() {
            let secrets = self.load_encrypted(&key)?;
            if let Some(first) = secrets.values().next() {
                // Try to decrypt the first secret to verify the key
                let decrypted = Self::decrypt_blob(&key, &first.encrypted_blob)?;
                // Verify checksum
                let checksum = crypto::sha256(&decrypted);
                if checksum != first.checksum {
                    return Err(VaultError::DecryptionFailed);
                }
            }
        }

        self.master_key = Some(key);
        Ok(())
    }

    /// Lock the vault, zeroizing the master key from memory.
    pub fn lock(&mut self) {
        if let Some(ref mut key) = self.master_key {
            use zeroize::Zeroize;
            key.zeroize();
        }
        self.master_key = None;
    }

    /// Store a secret in the vault (must be unlocked).
    pub fn store(&self, secret: Secret) -> Result<(), VaultError> {
        let key = self
            .master_key
            .as_ref()
            .ok_or(VaultError::DecryptionFailed)?;

        let mut secrets = if Path::new(&self.path).exists() {
            self.load_encrypted(key)?
        } else {
            HashMap::new()
        };

        secrets.insert(secret.id.clone(), secret);
        let json = serde_json::to_vec(&secrets).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;

        // Encrypt the entire secrets store
        let (nonce, ciphertext) = crypto::encrypt(key, &json);
        let mut blob = nonce;
        blob.extend_from_slice(&ciphertext);

        std::fs::write(&self.path, &blob)?;
        Ok(())
    }

    /// Encrypt and store a plaintext secret value.
    pub fn put(
        &self,
        id: &str,
        label: &str,
        kind: SecretKind,
        plaintext: &[u8],
    ) -> Result<(), VaultError> {
        let key = self
            .master_key
            .as_ref()
            .ok_or(VaultError::DecryptionFailed)?;

        let (nonce, ciphertext) = crypto::encrypt(key, plaintext);
        let mut blob = nonce;
        blob.extend_from_slice(&ciphertext);
        let checksum = crypto::sha256(plaintext);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let secret = Secret {
            id: id.to_string(),
            label: label.to_string(),
            kind,
            encrypted_blob: blob,
            checksum,
            created_at: now,
            updated_at: now,
        };

        self.store(secret)
    }

    /// Retrieve and decrypt a secret.
    pub fn get(&self, id: &str) -> Result<Vec<u8>, VaultError> {
        let key = self
            .master_key
            .as_ref()
            .ok_or(VaultError::DecryptionFailed)?;

        let secrets = self.load_encrypted(key)?;
        let secret = secrets
            .get(id)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "secret not found"))?;

        Self::decrypt_blob(key, &secret.encrypted_blob)
    }

    /// List all secret IDs without decrypting them.
    pub fn list_ids(&self) -> Result<Vec<String>, VaultError> {
        if !Path::new(&self.path).exists() {
            return Ok(vec![]);
        }
        let key = self
            .master_key
            .as_ref()
            .ok_or(VaultError::DecryptionFailed)?;
        let secrets = self.load_encrypted(key)?;
        Ok(secrets.keys().cloned().collect())
    }

    /// Delete a secret.
    pub fn delete(&self, id: &str) -> Result<(), VaultError> {
        let key = self
            .master_key
            .as_ref()
            .ok_or(VaultError::DecryptionFailed)?;
        let mut secrets = self.load_encrypted(key)?;
        secrets.remove(id);

        let json = serde_json::to_vec(&secrets).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;
        let (nonce, ciphertext) = crypto::encrypt(key, &json);
        let mut blob = nonce;
        blob.extend_from_slice(&ciphertext);
        std::fs::write(&self.path, &blob)?;

        Ok(())
    }

    // ── Private helpers ─────────────────────────────────────────────────

    fn load_encrypted(&self, key: &MasterKey) -> Result<HashMap<String, Secret>, VaultError> {
        let blob = std::fs::read(&self.path)?;
        Self::decrypt_secrets_map(key, &blob)
    }

    fn decrypt_blob(key: &MasterKey, blob: &[u8]) -> Result<Vec<u8>, VaultError> {
        if blob.len() < 12 {
            return Err(VaultError::DecryptionFailed);
        }
        let nonce = &blob[..12];
        let ciphertext = &blob[12..];
        crypto::decrypt(key, nonce, ciphertext)
    }

    fn decrypt_secrets_map(
        key: &MasterKey,
        blob: &[u8],
    ) -> Result<HashMap<String, Secret>, VaultError> {
        let json = Self::decrypt_blob(key, blob)?;
        serde_json::from_slice(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()).into())
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (Vault, TempDir) {
        let dir = TempDir::new().expect("tempdir");
        let vault = Vault::open(dir.path()).expect("open vault");
        (vault, dir)
    }

    #[test]
    fn test_vault_new_is_locked() {
        let (vault, _dir) = setup();
        assert!(!vault.is_unlocked());
    }

    #[test]
    fn test_unlock_lock_cycle() {
        let (mut vault, _dir) = setup();
        vault.unlock("test passphrase").expect("unlock");
        assert!(vault.is_unlocked());

        vault.lock();
        assert!(!vault.is_unlocked());
    }

    #[test]
    fn test_unlock_wrong_passphrase_fails() {
        let (mut vault, _dir) = setup();

        // First unlock to create the vault with this passphrase
        vault.unlock("correct").expect("initial unlock");
        vault.put("test", "test", SecretKind::Password, b"data").expect("put");
        vault.lock();

        // Try wrong passphrase
        let result = vault.unlock("wrong");
        assert!(result.is_err());
    }

    #[test]
    fn test_put_and_get() {
        let (mut vault, _dir) = setup();
        vault.unlock("passphrase").expect("unlock");

        vault
            .put("ssh-key-1", "My Server Key", SecretKind::SshKey, b"-----BEGIN OPENSSH PRIVATE KEY-----\nfake key content\n-----END OPENSSH PRIVATE KEY-----")
            .expect("put");

        let result = vault.get("ssh-key-1").expect("get");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_list_secrets() {
        let (mut vault, _dir) = setup();
        vault.unlock("pass").expect("unlock");

        vault.put("a", "First", SecretKind::Password, b"p1").expect("put");
        vault.put("b", "Second", SecretKind::SshKey, b"k2").expect("put");

        let ids = vault.list_ids().expect("list");
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"a".to_string()));
        assert!(ids.contains(&"b".to_string()));
    }

    #[test]
    fn test_delete_secret() {
        let (mut vault, _dir) = setup();
        vault.unlock("pass").expect("unlock");

        vault.put("x", "X", SecretKind::Password, b"px").expect("put");
        assert_eq!(vault.list_ids().unwrap().len(), 1);

        vault.delete("x").expect("delete");
        assert_eq!(vault.list_ids().unwrap().len(), 0);
        assert!(vault.get("x").is_err());
    }

    #[test]
    fn test_get_locked_fails() {
        let (mut vault, _dir) = setup();
        vault.unlock("pass").expect("unlock");
        vault
            .put("id", "label", SecretKind::Password, b"secret")
            .expect("put");
        vault.lock();

        assert!(vault.get("id").is_err());
    }

    #[test]
    fn test_put_locked_fails() {
        let (vault, _dir) = setup();
        assert!(vault
            .put("id", "label", SecretKind::Password, b"secret")
            .is_err());
    }

    #[test]
    fn test_vault_persistence() {
        let dir = TempDir::new().expect("tempdir");

        // Create and store
        {
            let mut vault = Vault::open(dir.path()).expect("open");
            vault.unlock("persistent").expect("unlock");
            vault
                .put("key1", "Key 1", SecretKind::SshKey, b"private-key-data")
                .expect("put");
        }

        // Reopen and retrieve
        {
            let mut vault = Vault::open(dir.path()).expect("reopen");
            vault.unlock("persistent").expect("unlock");
            let data = vault.get("key1").expect("get");
            assert_eq!(data, b"private-key-data");
        }
    }

    #[test]
    fn test_large_secret() {
        let (mut vault, _dir) = setup();
        vault.unlock("pass").expect("unlock");

        let big_data = vec![0xABu8; 100_000]; // 100 KB
        vault
            .put("big", "Big secret", SecretKind::Generic, &big_data)
            .expect("put");

        let retrieved = vault.get("big").expect("get");
        assert_eq!(retrieved, big_data);
    }
}
