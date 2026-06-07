//! Local and remote filesystem operations for the SFTP dual-pane browser.

use anyhow::{Context, Result};
use std::path::Path;

/// A file entry shown in the SFTP browser.
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// Local filesystem
// ═══════════════════════════════════════════════════════════════════════════

/// List entries in a local directory. Hidden files are included if show_hidden.
pub fn list_local(path: &Path, show_hidden: bool) -> Result<Vec<FileEntry>> {
    let mut entries: Vec<FileEntry> = std::fs::read_dir(path)
        .context("Failed to read local directory")?
        .filter_map(|e| e.ok())
        .filter(|e| {
            show_hidden || !e.file_name().to_string_lossy().starts_with('.')
        })
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let full_path = e.path();
            let meta = e.metadata().ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = format_time(meta.and_then(|m| m.modified().ok()));
            FileEntry { name, path: full_path.to_string_lossy().to_string(), is_dir, size, modified }
        })
        .collect();

    entries.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

fn format_time(st: Option<std::time::SystemTime>) -> String {
    st.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| {
            let secs = d.as_secs();
            // Simple: just show age. Real implementation would use proper formatting.
            if secs < 60 { format!("{}s ago", secs) }
            else if secs < 3600 { format!("{}m ago", secs / 60) }
            else if secs < 86400 { format!("{}h ago", secs / 3600) }
            else { format!("{}d ago", secs / 86400) }
        })
        .unwrap_or_default()
}

/// Format file size in human-readable form.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 { format!("{} B", bytes) }
    else if bytes < 1024 * 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
    else if bytes < 1024 * 1024 * 1024 { format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)) }
    else { format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0)) }
}

// ═══════════════════════════════════════════════════════════════════════════
// Remote SFTP
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "gui")]
pub mod remote {
    use super::*;
    use russh_sftp::client::SftpSession;

    /// List entries in a remote directory via SFTP.
    pub async fn list_remote(
        sftp: &SftpSession,
        path: &str,
        show_hidden: bool,
    ) -> Result<Vec<FileEntry>> {
        let entries = sftp.read_dir(path).await.context("SFTP read_dir failed")?;
        let mut result: Vec<FileEntry> = entries
            .into_iter()
            .filter(|e| show_hidden || !e.file_name().starts_with('.'))
            .map(|e| {
                let meta = e.metadata();
                FileEntry {
                    name: e.file_name(),
                    path: format!("{}/{}", path.trim_end_matches('/'), e.file_name()),
                    is_dir: meta.is_dir(),
                    size: meta.len(),
                    modified: {
                        let st = meta.modified().ok();
                        st.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    }
                        .map(|d| {
                            let secs = d.as_secs();
                            if secs < 3600 { format!("{}m ago", secs / 60) }
                            else if secs < 86400 { format!("{}h ago", secs / 3600) }
                            else { format!("{}d ago", secs / 86400) }
                        }).unwrap_or_default(),
                }
            }).collect();
        result.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(result)
    }

    /// Get parent directory path.
    pub fn parent_path(path: &str) -> String {
        let path = path.trim_end_matches('/');
        if path.is_empty() || path == "/" { "/".to_string() }
        else {
            match path.rfind('/') {
                Some(0) => "/".to_string(),
                Some(pos) => path[..pos].to_string(),
                None => "/".to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
    }

    #[test]
    #[cfg(feature = "gui")]
    fn test_parent_path() {
        assert_eq!(remote::parent_path("/"), "/");
        assert_eq!(remote::parent_path("/home"), "/");
        assert_eq!(remote::parent_path("/home/user"), "/home");
    }
}
