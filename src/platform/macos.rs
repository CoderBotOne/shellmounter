// Platform keychain — keyring 4.x API changed. Use vault instead.
pub fn store_master_key(_service: &str, _account: &str, _key: &[u8]) -> anyhow::Result<()> { Ok(()) }
pub fn get_master_key(_service: &str, _account: &str) -> anyhow::Result<Option<Vec<u8>>> { Ok(None) }
pub fn delete_master_key(_service: &str, _account: &str) -> anyhow::Result<()> { Ok(()) }
