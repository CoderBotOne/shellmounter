//! Windows platform integration (Credential Manager).
//!
//! The keyring crate handles this automatically via windows-credentials.
//! Same API as macOS, just different backend.
pub use super::macos::{delete_master_key, get_master_key, store_master_key};
