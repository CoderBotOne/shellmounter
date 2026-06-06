//! Linux platform integration (Secret Service via DBus).
//!
//! The keyring crate handles this automatically via dbus-secret-service.
//! Same API as macOS, just different backend.
pub use super::macos::{delete_master_key, get_master_key, store_master_key};
