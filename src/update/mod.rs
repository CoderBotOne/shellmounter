//! Cloudflare R2 auto-update system. Checks manifest, downloads, verifies SHA-256.

use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const MANIFEST_URL: &str = "https://pub-REPLACE_WITH_YOUR.r2.dev/shellmounter/version.json";
const CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Deserialize, Debug)]
struct Manifest {
    version: String,
    sha256: String,
    url: String,
    notes: String,
}

/// Check for updates synchronously. Returns Ok(true) if updated.
pub fn check() -> Result<bool> {
    let current = env!("CARGO_PKG_VERSION");

    let client = reqwest::blocking::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .user_agent(concat!("ShellMounter/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("HTTP client")?;

    let resp = client.get(MANIFEST_URL).send().context("fetch manifest")?;

    if !resp.status().is_success() {
        log::debug!("Update check: HTTP {}", resp.status());
        return Ok(false);
    }

    let manifest: Manifest = resp.json().context("invalid manifest")?;

    if manifest.version == current {
        log::debug!("Already latest v{current}");
        return Ok(false);
    }

    log::info!("Update: v{current} → v{}", manifest.version);

    // Download binary
    let resp = client.get(&manifest.url).send().context("download")?;
    let data = resp.bytes().context("read bytes")?;

    // Verify SHA-256
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual_hex = hex::encode(hasher.finalize());

    if actual_hex != manifest.sha256 {
        anyhow::bail!("Checksum mismatch! Expected {}, got {}", manifest.sha256, actual_hex);
    }

    log::info!("SHA-256 verified. {} bytes downloaded.", data.len());
    log::info!("Changelog: {}", manifest.notes);

    // Self-replace on unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let current_exe = std::env::current_exe()?;
        let parent = current_exe.parent().context("no parent")?;
        let tmp = parent.join(".shellmounter.new");
        std::fs::write(&tmp, &data)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&tmp, &current_exe)?;
    }

    #[cfg(windows)]
    {
        let current_exe = std::env::current_exe()?;
        let parent = current_exe.parent().context("no parent")?;
        let new_path = parent.join("shellmounter.new.exe");
        std::fs::write(&new_path, &data)?;
        log::info!("New binary: {}", new_path.display());
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_parse() {
        let json = r#"{"version":"0.2.0","sha256":"abc","url":"http://x","notes":"fixes"}"#;
        let m: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.version, "0.2.0");
    }

    #[test]
    fn test_verify_hash() {
        let hash = hex::encode(Sha256::digest(b"hello"));
        assert_eq!(hash.len(), 64);
    }
}
