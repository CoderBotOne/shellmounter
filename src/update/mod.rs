//! Cloudflare R2 update system
//!
//! Checks a version manifest on Cloudflare R2 and downloads new releases.
//! Uses reqwest + rustls (zero OpenSSL dependency).

use anyhow::Result;
use serde::Deserialize;

/// Version manifest hosted on Cloudflare R2
/// URL: https://pub-<hash>.r2.dev/shellmounter/version.json
const UPDATE_URL: &str = "https://pub-REPLACE_WITH_YOUR.r2.dev/shellmounter/version.json";

#[derive(Deserialize)]
struct Manifest {
    version: String,
    sha256: String,
    url: String,
    notes: String,
}

/// Check for updates (non-blocking, called from spawn thread)
pub fn check() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp = client.get(UPDATE_URL).send()?;
    let manifest: Manifest = resp.json()?;

    if manifest.version != current {
        log::info!(
            "Update available: v{} → v{} — {}",
            current,
            manifest.version,
            manifest.notes
        );
        // TODO: download and verify SHA256, then self-replace
    } else {
        log::debug!("Already on latest version v{current}");
    }

    Ok(())
}
