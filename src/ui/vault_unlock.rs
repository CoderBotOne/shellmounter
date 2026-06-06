//! Vault unlock dialog state.

/// Vault unlock dialog state.
#[derive(Default)]
pub struct VaultUnlockState {
    pub password: String,
    pub error: Option<String>,
    pub attempts: u32,
}
