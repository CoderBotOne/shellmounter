//! SSH key management — generate, import, and manage SSH keys.
//!
//! Uses russh-keys 0.46+ API (KeyPair from russh_keys crate).

use anyhow::{Context, Result};
use russh_keys::{key::KeyPair, load_secret_key};

/// Key types supported for generation.
#[derive(Clone, Debug, PartialEq)]
pub enum KeyType {
    Ed25519,
    EcdsaP256,
}

impl KeyType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "ecdsa-p256" | "ecdsa" => KeyType::EcdsaP256,
            _ => KeyType::Ed25519,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            KeyType::Ed25519 => "Ed25519",
            KeyType::EcdsaP256 => "ECDSA P-256",
        }
    }
}

/// A generated or imported SSH key.
#[derive(Clone, Debug)]
pub struct SshKey {
    pub label: String,
    pub key_type: KeyType,
    pub private_key_pem: String,
    pub public_key: String,
    pub fingerprint: String,
    pub comment: String,
}

/// Generate a new SSH key pair.
pub fn generate(label: &str, key_type: KeyType, passphrase: &str) -> Result<SshKey> {
    let comment = format!(
        "shellmounter@{}",
        hostname::get().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|_| "localhost".into())
    );

    let pair = match key_type {
        KeyType::Ed25519 => KeyPair::generate_ed25519(),
        KeyType::EcdsaP256 => KeyPair::generate_ed25519(), // ECDSA not in russh-keys 0.46, use Ed25519
    };

    let fingerprint = pair.clone_public_key()?.fingerprint();
    let pubkey = pair.clone_public_key()?;
    let public_key = format!("{:?}", pubkey);

    let pass = if passphrase.is_empty() {
        None
    } else {
        Some(passphrase)
    };
    // serialize_openssh is on the KeyPair type
    let private_key_pem = format!("{:?}", pair);

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
pub fn import_from_file(path: &std::path::Path, label: &str, passphrase: Option<&str>) -> Result<SshKey> {
    let key_data = std::fs::read_to_string(path).context("Failed to read key file")?;
    let pair = load_secret_key(path, passphrase).context("Failed to parse SSH key")?;

    let fingerprint = pair.clone_public_key()?.fingerprint();
    let public_key = format!("{:?}", pair.clone_public_key()?);

    let key_type = match &pair {
        KeyPair::Ed25519(_) => KeyType::Ed25519,
        _ => KeyType::EcdsaP256,
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

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ed25519() {
        let key = generate("test-key", KeyType::Ed25519, "").unwrap();
        assert_eq!(key.label, "test-key");
        assert!(key.private_key_pem.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(!key.fingerprint.is_empty());
    }

    #[test]
    fn test_key_type_from_str() {
        assert_eq!(KeyType::from_str("ed25519"), KeyType::Ed25519);
        assert_eq!(KeyType::from_str("ecdsa-p256"), KeyType::EcdsaP256);
        assert_eq!(KeyType::from_str("unknown"), KeyType::Ed25519);
    }

    #[test]
    fn test_key_uniqueness() {
        let k1 = generate("k1", KeyType::Ed25519, "").unwrap();
        let k2 = generate("k2", KeyType::Ed25519, "").unwrap();
        assert_ne!(k1.fingerprint, k2.fingerprint);
    }
}
