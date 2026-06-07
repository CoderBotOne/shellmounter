//! SSH key management — generate, import, and manage SSH keys.
//!
//! Uses russh-keys 0.46+ API (KeyPair from russh_keys crate).
//! Keys are stored as raw bytes (Ed25519: 32-byte secret) so they can be
//! reconstructed and used for authentication without relying on Debug format.

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use russh_keys::{key::KeyPair, load_secret_key};

/// Key types supported for generation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum KeyType {
    Ed25519,
    /// P-256 is generated as Ed25519 — russh-keys 0.46 ships Ed25519 only.
    /// The label is preserved so the UI displays the user's choice accurately.
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
            KeyType::EcdsaP256 => "ECDSA P-256 (→ Ed25519)",
        }
    }
}

/// A generated or imported SSH key.
///
/// `private_key_bytes` holds the raw Ed25519 secret (32 bytes) encoded as hex.
/// This can be reconstructed into a `KeyPair` via `keypair_from_bytes()`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SshKey {
    pub label: String,
    pub key_type: KeyType,
    /// Hex-encoded raw Ed25519 secret key (64 hex chars = 32 bytes).
    pub private_key_bytes: String,
    /// OpenSSH-format public key line (ssh-ed25519 AAAA… comment).
    pub public_key: String,
    /// SHA-256 fingerprint.
    pub fingerprint: String,
    pub comment: String,
}

// ── KeyPair ↔ bytes conversion ────────────────────────────────────────

/// Serialise an Ed25519 KeyPair to raw 32-byte secret.
pub fn keypair_to_bytes(pair: &KeyPair) -> Result<Vec<u8>> {
    match pair {
        KeyPair::Ed25519(sk) => Ok(sk.to_bytes().to_vec()),
        _ => anyhow::bail!("Only Ed25519 keys are supported in russh-keys 0.46"),
    }
}

/// Reconstruct an Ed25519 KeyPair from raw 32-byte secret.
pub fn keypair_from_bytes(bytes: &[u8]) -> Result<KeyPair> {
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Ed25519 secret key must be exactly 32 bytes"))?;
    let signing_key = SigningKey::from_bytes(&arr);
    Ok(KeyPair::Ed25519(signing_key))
}

/// Produce an OpenSSH-format public key line: "ssh-ed25519 <base64> <comment>"
pub fn public_key_openssh(pair: &KeyPair, comment: &str) -> Result<String> {
    match pair {
        KeyPair::Ed25519(sk) => {
            let verifying_key = sk.verifying_key();
            let raw_pub = verifying_key.as_bytes(); // 32 bytes
            let encoded = base64_encode_openssh(raw_pub);
            Ok(format!("ssh-ed25519 {} {}", encoded, comment))
        }
        _ => anyhow::bail!("Unsupported key type for OpenSSH export"),
    }
}

/// Base64-encode a byte slice in the style OpenSSH expects (standard base64).
fn base64_encode_openssh(data: &[u8]) -> String {

    // RFC 4648 base64 alphabet
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let chunks = data.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        result.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[(triple & 0x3F) as usize] as char);
        }
    }

    // Pad
    let rem = data.len() % 3;
    if rem == 1 {
        result.push('=');
        result.push('=');
    } else if rem == 2 {
        result.push('=');
    }

    result
}

/// Generate a new SSH key pair.
pub fn generate(label: &str, key_type: KeyType, _passphrase: &str) -> Result<SshKey> {
    let comment = format!(
        "shellmounter@{}",
        hostname::get()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "localhost".into())
    );

    // russh-keys 0.46 only supports Ed25519 generation.
    // ECDSA P-256 is displayed accurately but generates Ed25519 underneath.
    let pair = KeyPair::generate_ed25519();

    let fingerprint = pair.clone_public_key()?.fingerprint();
    let public_key = public_key_openssh(&pair, &comment)
        .unwrap_or_else(|_| format!("ssh-ed25519 <error> {}", comment));

    // Serialise the private key to raw bytes (32 bytes → 64 hex chars)
    let raw = keypair_to_bytes(&pair)?;
    let private_key_bytes = hex::encode(&raw);

    Ok(SshKey {
        label: label.to_string(),
        key_type,
        private_key_bytes,
        public_key,
        fingerprint,
        comment,
    })
}

/// Import an existing SSH key from a file path.
/// Uses russh_keys::load_secret_key which handles OpenSSH and PEM formats.
pub fn import_from_file(
    path: &std::path::Path,
    label: &str,
    passphrase: Option<&str>,
) -> Result<SshKey> {
    let pair = load_secret_key(path, passphrase).context("Failed to parse SSH key")?;

    let fingerprint = pair.clone_public_key()?.fingerprint();
    let comment = format!("imported from {}", path.display());
    let public_key = public_key_openssh(&pair, &comment)
        .unwrap_or_else(|_| format!("<error> {}", comment));

    let raw = keypair_to_bytes(&pair)?;
    let private_key_bytes = hex::encode(&raw);

    let key_type = match &pair {
        KeyPair::Ed25519(_) => KeyType::Ed25519,
        _ => KeyType::EcdsaP256,
    };

    Ok(SshKey {
        label: label.to_string(),
        key_type,
        private_key_bytes,
        public_key,
        fingerprint,
        comment,
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
        assert_eq!(key.private_key_bytes.len(), 64); // 32 bytes → 64 hex
        assert!(!key.fingerprint.is_empty());
        assert!(key.public_key.starts_with("ssh-ed25519 "));
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
        assert_ne!(k1.private_key_bytes, k2.private_key_bytes);
    }

    #[test]
    fn test_roundtrip_bytes() {
        let pair = KeyPair::generate_ed25519();
        let bytes = keypair_to_bytes(&pair).unwrap();
        assert_eq!(bytes.len(), 32);
        let reconstructed = keypair_from_bytes(&bytes).unwrap();
        let orig_fp = pair.clone_public_key().unwrap().fingerprint();
        let recon_fp = reconstructed.clone_public_key().unwrap().fingerprint();
        assert_eq!(orig_fp, recon_fp);
    }

    #[test]
    fn test_public_key_openssh() {
        let pair = KeyPair::generate_ed25519();
        let pk = public_key_openssh(&pair, "test").unwrap();
        assert!(pk.starts_with("ssh-ed25519 "));
        assert!(pk.ends_with(" test"));
        // Should have valid base64 in the middle
        let parts: Vec<&str> = pk.split_whitespace().collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "ssh-ed25519");
    }
}
