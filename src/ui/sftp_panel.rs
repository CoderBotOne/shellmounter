//! SFTP file browser panel.

use serde::{Deserialize, Serialize};

/// An entry in the SFTP file browser.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SftpFileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

/// SFTP panel state.
pub struct SftpPanelState {
    pub current_path: String,
    pub entries: Vec<SftpFileEntry>,
    pub selected: Option<String>,
    pub loading: bool,
}

impl Default for SftpPanelState {
    fn default() -> Self {
        Self {
            current_path: "/".into(),
            entries: vec![],
            selected: None,
            loading: false,
        }
    }
}
