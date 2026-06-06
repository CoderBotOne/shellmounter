//! macOS platform integration.
//!
//! - Keychain access for master vault password
//! - Menubar icon (optional)
//! - Open at login (LaunchAgent)

use anyhow::Result;

/// Store the vault master key in macOS Keychain.
pub fn store_master_key(service: &str, account: &str, key: &[u8]) -> Result<()> {
    let entry = keyring::Entry::new(service, account)?;
    let encoded = hex::encode(key);
    entry.set_password(&encoded)?;
    Ok(())
}

/// Retrieve the vault master key from macOS Keychain.
pub fn get_master_key(service: &str, account: &str) -> Result<Option<Vec<u8>>> {
    let entry = keyring::Entry::new(service, account)?;
    match entry.get_password() {
        Ok(encoded) => {
            let key = hex::decode(&encoded)?;
            Ok(Some(key))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Delete the vault master key from macOS Keychain.
pub fn delete_master_key(service: &str, account: &str) -> Result<()> {
    let entry = keyring::Entry::new(service, account)?;
    entry.delete_credential()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_roundtrip() {
        let key = b"test-master-key-32-bytes-xxxxxx";
        let service = "com.shellmounter.test";

        // Store
        store_master_key(service, "test-user", key).ok();

        // Retrieve
        if let Ok(Some(retrieved)) = get_master_key(service, "test-user") {
            assert_eq!(retrieved, key);
            // Cleanup
            delete_master_key(service, "test-user").ok();
        }
    }
}
