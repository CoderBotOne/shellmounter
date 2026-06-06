//! SFTP client built on russh-sftp.
//!
//! Provides file listing, upload, download, and directory operations.

use anyhow::{Context, Result};
use russh_sftp::client::SftpSession;

/// List files in a remote directory.
pub async fn list(sftp: &SftpSession, path: &str) -> Result<Vec<SftpEntry>> {
    let entries = sftp
        .read_dir(path)
        .await
        .context("Failed to list remote directory")?;

    Ok(entries
        .into_iter()
        .map(|e| SftpEntry {
            name: e.file_name(),
            is_dir: e.metadata().is_dir(),
            size: e.metadata().len().unwrap_or(0),
        })
        .collect())
}

/// A simplified SFTP entry for UI rendering.
#[derive(Debug, Clone)]
pub struct SftpEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_entry_fields() {
        let entry = super::SftpEntry {
            name: "test.txt".to_string(),
            is_dir: false,
            size: 1024,
        };
        assert_eq!(entry.name, "test.txt");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, 1024);
    }
}
