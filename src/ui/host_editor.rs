//! Host editor modal component.
//! Form fields: label, hostname, port, username, auth method, group, tags.

use crate::db::hosts::AuthMethod;

/// Host form data for the editor.
#[derive(Clone, Debug, Default)]
pub struct HostForm {
    pub label: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub auth_method: AuthMethod,
    pub group: Option<String>,
    pub tags: Vec<String>,
}
