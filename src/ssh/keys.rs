//! SSH key management — generate, import, and manage SSH keys.
//!
//! Uses russh-keys for ED25519, RSA, and ECDSA key generation.
//! Generated keys are stored in the encrypted vault.

use anyhow::{Context, Result};
use rand::rngs::OsRng;
use russh_keys::{key::KeyPair, load_secret_key, PrivateKeyWithHashAlg};
use std::path::Path;

/// Key types supported for generation.
#[derive(Clone, Debug, PartialEq)]
pub enum KeyType {
    /// Ed25519 (recommended — fast, secure, small)
    Ed25519,
    /// RSA 4096-bit
    Rsa4096,
    /// ECDSA P-256
    EcdsaP256,
}

impl KeyType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "rsa-4096" | "rsa" => KeyType::Rsa4096,
            "ecdsa-p256" | "ecdsa" => KeyType::EcdsaP256,
            _ => KeyType::Ed25519,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            KeyType::Ed25519 => "Ed25519",
            KeyType::Rsa4096 => "RSA 4096",
            KeyType::EcdsaP256 => "ECDSA P-256",
        }
    }
}

/// A generated or imported SSH key.
#[derive(Clone, Debug)]
pub struct SshKey {
    /// Label for display in UI
    pub label: String,
    /// Key type
    pub key_type: KeyType,
    /// Private key in OpenSSH format (PEM)
    pub private_key_pem: String,
    /// Public key in OpenSSH format
    pub public_key: String,
    /// SHA-256 fingerprint
    pub fingerprint: String,
    /// Comment (default: "shellmounter@hostname")
    pub comment: String,
}

/// Generate a new SSH key pair.
///
/// # Example
/// ```ignore
/// let key = KeyManager::generate("My Server Key", KeyType::Ed25519, "")?;
/// println!("Fingerprint: {}", key.fingerprint);
/// ```
pub fn generate(label: &str, key_type: KeyType, passphrase: &str) -> Result<SshKey> {
    let comment = format!(
        "shellmounter@{}",
        hostname::get().unwrap_or_else(|_| "localhost".into())
    );

    let key_pair = match key_type {
        KeyType::Ed25519 => {
            let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
            let pair = KeyPair::Ed25519(key);
            pair
        }
        KeyType::Rsa4096 => {
            let mut rng = OsRng;
            let key = rsa::RsaPrivateKey::new(&mut rng, 4096).context("Failed to generate RSA key")?;
            KeyPair::Rsa {
                key: std::sync::Arc::new(key),
                hash_alg: Some(SignatureHashAlg::SHA2_512),
            }
        }
        KeyType::EcdsaP256 => {
            let key = p256::SecretKey::random(&mut OsRng);
            KeyPair::Ecdsa256(key)
        }
    };

    let fingerprint = key_pair.fingerprint();

    // Serialize private key
    let private_key_pem = key_pair.serialize_openssh(
        if passphrase.is_empty() { None } else { Some(passphrase) },
        &comment,
    )?;

    // Serialize public key
    let public_key = key_pair.serialize_public_key()?;

    Ok(SshKey {
        label: label.to_string(),
        key_type,
        private_key_pem,
        public_key,
        fingerprint,
        comment,
    })
}

/// Import an existing SSH key from a file path.
pub fn import_from_file(path: &Path, label: &str, passphrase: Option<&str>) -> Result<SshKey> {
    let key_data = std::fs::read_to_string(path)
        .context("Failed to read key file")?;

    let key_pair = load_secret_key(path, passphrase)
        .context("Failed to parse SSH key")?;

    let fingerprint = key_pair.fingerprint();
    let public_key = key_pair.serialize_public_key()?;

    // Determine key type
    let key_type = match &key_pair {
        KeyPair::Ed25519(_) => KeyType::Ed25519,
        KeyPair::Rsa { .. } => KeyType::Rsa4096,
        KeyPair::Ecdsa256(_) => KeyType::EcdsaP256,
    };

    Ok(SshKey {
        label: label.to_string(),
        key_type,
        private_key_pem: key_data,
        public_key,
        fingerprint,
        comment: "imported".into(),
    })
}

// Re-export types from russh-keys for internal use
use rsa::RsaPrivateKey;
use russh_keys::SignatureHashAlg;
use p256::SecretKey;

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ed25519() {
        let key = generate("test-key", KeyType::Ed25519, "").unwrap();
        assert_eq!(key.label, "test-key");
        assert!(!key.private_key_pem.is_empty());
        assert!(key.private_key_pem.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(key.public_key.starts_with("ssh-ed25519 "));
        assert!(!key.fingerprint.is_empty());
    }

    #[test]
    fn test_generate_ed25519_with_passphrase() {
        let key = generate("protected-key", KeyType::Ed25519, "my-passphrase").unwrap();
        assert!(key.private_key_pem.contains("ENCRYPTED"));
    }

    #[test]
    fn test_key_type_from_str() {
        assert_eq!(KeyType::from_str("ed25519"), KeyType::Ed25519);
        assert_eq!(KeyType::from_str("rsa-4096"), KeyType::Rsa4096);
        assert_eq!(KeyType::from_str("ecdsa-p256"), KeyType::EcdsaP256);
        assert_eq!(KeyType::from_str("unknown"), KeyType::Ed25519); // default
    }

    #[test]
    fn test_display_name() {
        assert_eq!(KeyType::Ed25519.display_name(), "Ed25519");
        assert_eq!(KeyType::Rsa4096.display_name(), "RSA 4096");
        assert_eq!(KeyType::EcdsaP256.display_name(), "ECDSA P-256");
    }

    #[test]
    fn test_key_uniqueness() {
        let key1 = generate("k1", KeyType::Ed25519, "").unwrap();
        let key2 = generate("k2", KeyType::Ed25519, "").unwrap();
        assert_ne!(key1.fingerprint, key2.fingerprint, "Keys must be unique");
        assert_ne!(key1.public_key, key2.public_key);
    }
}
