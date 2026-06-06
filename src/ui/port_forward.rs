//! Port forwarding panel.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PortForward {
    pub id: String,
    pub label: String,
    pub kind: ForwardKind,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ForwardKind {
    Local,
    Remote,
    Dynamic,
}
