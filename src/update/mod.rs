//! Cloudflare R2 auto-update system.
//!
//! On startup, checks a version manifest hosted on R2.
//! If a new version is available, downloads and verifies the binary,
//! then replaces itself atomically.

use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::Path;

/// URL of the version manifest on Cloudflare R2.
/// Replace with your actual R2 public bucket URL.
const MANIFEST_URL: &str = "https://pub-REPLACE_WITH_YOUR.r2.dev/shellmounter/version.json";

/// Duration to wait for the update check before timing out.
const CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Version manifest hosted on R2.
#[derive(Deserialize, Debug)]
struct Manifest {
    /// Semver version string (e.g., "0.2.0")
    version: String,
    /// SHA-256 hex digest of the release binary
    sha256: String,
    /// Direct download URL for the binary
    url: String,
    /// Release notes (markdown)
    notes: String,
    /// Minimum supported OS version (optional)
    #[serde(default)]
    min_os: Option<String>,
}

/// Check for updates synchronously (designed to run in a background thread).
///
/// Returns Ok(true) if an update was found and applied.
/// Returns Ok(false) if already on latest.
pub fn check() -> Result<bool> {
    let current = env!("CARGO_PKG_VERSION");

    let client = reqwest::blocking::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .user_agent(concat!("ShellMounter/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("Failed to build HTTP client")?;

    // Fetch manifest
    let resp = client
        .get(MANIFEST_URL)
        .send()
        .context("Failed to fetch update manifest")?;

    if !resp.status().is_success() {
        log::debug!("Update check returned HTTP {}", resp.status());
        return Ok(false);
    }

    let manifest: Manifest = resp.json().context("Invalid manifest format")?;

    // Compare versions (simple semver string comparison)
    if manifest.version == current {
        log::debug!("Already on latest version v{current}");
        return Ok(false);
    }

    log::info!(
        "Update available: v{current} → v{}",
        manifest.version
    );
    log::info!("Release notes: {}", manifest.notes);

    // Download the new binary
    let binary_data = download_binary(&client, &manifest)?;

    // Verify SHA-256
    verify_checksum(&binary_data, &manifest.sha256)?;

    // Atomic self-replace
    self_replace(&binary_data)?;

    log::info!("Update successful! Restart to use v{}", manifest.version);
    Ok(true)
}

/// Download the release binary from R2.
fn download_binary(client: &reqwest::blocking::Client, manifest: &Manifest) -> Result<Vec<u8>> {
    let resp = client
        .get(&manifest.url)
        .send()
        .context("Failed to download update")?;

    let total = resp.content_length().unwrap_or(0);
    let mut data = Vec::with_capacity(total as usize);

    // Stream download with progress logging
    let mut downloaded: u64 = 0;
    for chunk in resp {
        let chunk = chunk.context("Download interrupted")?;
        downloaded += chunk.len() as u64;
        data.extend_from_slice(&chunk);

        if total > 0 && downloaded % (1024 * 1024) == 0 {
            let pct = (downloaded * 100) / total;
            log::debug!("Download: {}% ({}/{})", pct, downloaded, total);
        }
    }

    log::info!("Downloaded {} bytes", data.len());
    Ok(data)
}

/// Verify the SHA-256 checksum of the downloaded binary.
fn verify_checksum(data: &[u8], expected_hex: &str) -> Result<()> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    let actual_hex = hex::encode(&hash[..]);

    if actual_hex != expected_hex {
        anyhow::bail!(
            "Checksum mismatch!\n  Expected: {}\n  Got:      {}",
            expected_hex,
            actual_hex
        );
    }

    log::debug!("SHA-256 verified: {}", actual_hex);
    Ok(())
}

/// Replace the current binary with the new one atomically.
/// On Unix: write to a temp file, rename over current executable.
/// On Windows: write to .new, use a batch script to swap on next restart.
fn self_replace(new_data: &[u8]) -> Result<()> {
    let current_exe = std::env::current_exe().context("Cannot find current executable")?;
    let parent = current_exe
        .parent()
        .context("Cannot find executable directory")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let temp_path = parent.join(".shellmounter.new");
        std::fs::write(&temp_path, new_data).context("Failed to write new binary")?;

        // Preserve executable permissions
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&temp_path, perms)?;

        // Atomic rename
        std::fs::rename(&temp_path, &current_exe).context("Failed to replace binary")?;

        log::info!("Binary replaced. Restart to apply.");
    }

    #[cfg(windows)]
    {
        let new_path = parent.join("shellmounter.new.exe");
        std::fs::write(&new_path, new_data).context("Failed to write new binary")?;

        log::info!(
            "New binary written to {}. Restart the app to use it.",
            new_path.display()
        );
    }

    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_checksum_valid() {
        let data = b"hello world";
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert!(verify_checksum(data, expected).is_ok());
    }

    #[test]
    fn test_verify_checksum_invalid() {
        let data = b"hello world";
        let expected = "0000000000000000000000000000000000000000000000000000000000000000";
        assert!(verify_checksum(data, expected).is_err());
    }

    #[test]
    fn test_verify_checksum_empty() {
        let data = b"";
        let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(verify_checksum(data, expected).is_ok());
    }

    #[test]
    fn test_manifest_parsing() {
        let json = r#"{
            "version": "0.2.0",
            "sha256": "abc123",
            "url": "https://example.com/shellmounter-v0.2.0",
            "notes": "Bug fixes and performance improvements"
        }"#;

        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.version, "0.2.0");
        assert_eq!(manifest.notes, "Bug fixes and performance improvements");
    }
}
