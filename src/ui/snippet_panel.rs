//! Snippet library for storing frequently-used commands.

use serde::{Deserialize, Serialize};

/// A saved command snippet.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub label: String,
    pub command: String,
    pub description: String,
    pub tags: Vec<String>,
}
