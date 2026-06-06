//! Host tree sidebar types and helpers.
//! The sidebar UI is rendered inline in app.rs for direct state access.
//! This module provides reusable data structures.

use serde::{Deserialize, Serialize};

/// A node in the host tree (group → hosts).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostTreeNode {
    pub id: String,
    pub label: String,
    pub kind: TreeNodeKind,
    pub children: Vec<HostTreeNode>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TreeNodeKind {
    Group,
    Host { host_id: String, connected: bool },
}
