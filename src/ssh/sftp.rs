//! SFTP client built on russh-sftp. File listing, upload, download.

use anyhow::{Context, Result};
use russh_sftp::client::SftpSession;

pub async fn list(sftp: &SftpSession, path: &str) -> Result<Vec<SftpEntry>> {
    let entries = sftp.read_dir(path).await.context("SFTP list failed")?;
    Ok(entries
        .into_iter()
        .map(|e| SftpEntry {
            name: e.file_name(),
            is_dir: e.metadata().is_dir(),
            size: Some(e.metadata().len()),
        })
        .collect())
}

#[derive(Debug, Clone)]
pub struct SftpEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_entry() {
        let e = super::SftpEntry { name: "f".into(), is_dir: false, size: Some(1024) };
        assert_eq!(e.name, "f");
        assert_eq!(e.size, Some(1024));
    }
}
