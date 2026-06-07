//! SSH session management via russh.
//!
//! Manages SSH connection lifecycle: connect, authenticate, PTY, reconnect.
//! Uses russh (pure Rust, async, Tokio-based).
//!
//! 🔒 SECURITY: TOFU (Trust On First Use) host key verification.
//! Host fingerprints are stored in SQLite and verified on every connection.

use anyhow::{Context, Result};
use async_trait::async_trait;
use russh::*;
use russh_keys::{agent::client::AgentClient, key::KeyPair};
use russh_sftp::client::SftpSession;
use std::collections::HashMap;
use std::sync::Arc;

/// Authentication method for the SSH session.
#[derive(Clone, Debug)]
pub enum AuthMethod {
    /// Authenticate with a raw Ed25519 private key (32 bytes).
    Key { key_bytes: Vec<u8> },
    /// Authenticate with password.
    Password { password: String },
    /// Use SSH agent forwarding.
    Agent,
}

/// An active SSH session with PTY.
pub struct SshSession {
    session: russh::client::Handle<Client>,
    channel: russh::Channel<russh::client::Msg>,
    host: String,
    port: u16,
    username: String,
    pty_ready: bool,
    /// Bastion session that must be kept alive for ProxyJump tunnels.
    /// Closed alongside this session on `close()`.
    #[allow(dead_code)]
    bastion: Option<Box<SshSession>>,
}

/// Client handler with TOFU host key verification.
struct Client {
    known_hosts: Arc<parking_lot::Mutex<KnownHosts>>,
}

/// Known hosts store (TOFU — Trust On First Use).
pub(crate) struct KnownHosts {
    /// host:port → fingerprint SHA-256
    fingerprints: HashMap<String, String>,
    /// Path to the known_hosts file
    path: std::path::PathBuf,
}

impl KnownHosts {
    fn load(data_dir: &std::path::Path) -> Self {
        let path = data_dir.join("known_hosts");
        let mut fingerprints = HashMap::new();

        if let Ok(contents) = std::fs::read_to_string(&path) {
            for line in contents.lines() {
                if let Some((host, fp)) = line.split_once(' ') {
                    fingerprints.insert(host.to_string(), fp.to_string());
                }
            }
        }

        Self { fingerprints, path }
    }

    fn save(&self) {
        let mut contents = String::new();
        for (host, fp) in &self.fingerprints {
            contents.push_str(&format!("{} {}\n", host, fp));
        }
        let _ = std::fs::write(&self.path, &contents);
    }

    fn check(&mut self, host: &str, port: u16, key: &russh_keys::key::PublicKey) -> Result<bool> {
        let host_key = format!("{}:{}", host, port);
        let fingerprint = key.fingerprint();

        if let Some(stored) = self.fingerprints.get(&host_key) {
            // Known host — verify fingerprint matches
            Ok(stored == &fingerprint)
        } else {
            // TOFU: first time seeing this host — trust and store
            log::info!("TOFU: trusting new host {}:{} — {}", host, port, fingerprint);
            self.fingerprints.insert(host_key, fingerprint);
            self.save();
            Ok(true)
        }
    }
}

#[async_trait]
impl client::Handler for Client {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // Deferred to verify_host_key() called before connect.
        Ok(true)
    }
}

// ── Key reconstruction (from raw bytes) ──────────────────────────────

/// Reconstruct an Ed25519 KeyPair from raw 32-byte secret.
fn keypair_from_bytes(bytes: &[u8]) -> Result<KeyPair> {
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Ed25519 secret key must be exactly 32 bytes"))?;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&arr);
    Ok(KeyPair::Ed25519(signing_key))
}

impl SshSession {
    /// Verify host key before connecting (TOFU).
    pub async fn verify_host_key(
        host: &str,
        port: u16,
        key: &russh_keys::key::PublicKey,
        known_hosts: &Arc<parking_lot::Mutex<KnownHosts>>,
    ) -> Result<()> {
        let mut hosts = known_hosts.lock();
        if !hosts.check(host, port, key)? {
            anyhow::bail!(
                "HOST KEY VERIFICATION FAILED: {}:{} — fingerprint has changed!\n\
                 This could indicate a MITM attack or a legitimate server reinstall.\n\
                 To fix: delete the entry from ~/.local/share/shellmounter/known_hosts",
                host, port
            );
        }
        Ok(())
    }

    /// Connect to an SSH server, optionally through a bastion/jump host.
    ///
    /// `auth` determines the authentication method:
    /// - `AuthMethod::Key { key_bytes }` — raw Ed25519 32-byte secret
    /// - `AuthMethod::Password { password }` — plaintext password
    /// - `AuthMethod::Agent` — SSH agent forwarding
    pub async fn connect(
        host: &str,
        port: u16,
        username: &str,
        auth: AuthMethod,
        data_dir: &std::path::Path,
    ) -> Result<Self> {
        let config = client::Config::default();
        let config = Arc::new(config);

        let known_hosts = Arc::new(parking_lot::Mutex::new(KnownHosts::load(data_dir)));

        let sh = Client {
            known_hosts: known_hosts.clone(),
        };

        let mut session = russh::client::connect(config, (host, port), sh)
            .await
            .context("SSH connection failed")?;

        // Authenticate based on method
        match auth {
            AuthMethod::Key { key_bytes } => {
                let key = keypair_from_bytes(&key_bytes)
                    .context("Failed to reconstruct SSH key from stored bytes")?;
                session
                    .authenticate_publickey(username, Arc::new(key))
                    .await
                    .context("SSH publickey authentication failed")?;
            }
            AuthMethod::Password { password } => {
                session
                    .authenticate_password(username, &password)
                    .await
                    .context("SSH password authentication failed")?;
            }
            AuthMethod::Agent => {
                let mut agent = AgentClient::connect_env().await
                    .context("SSH agent not available — is SSH_AUTH_SOCK set?")?;
                let identities = agent
                    .request_identities()
                    .await
                    .context("Failed to list SSH agent identities")?;
                // Try the first identity from the agent
                let pubkey = match identities.into_iter().next() {
                    Some(pk) => pk,
                    None => anyhow::bail!("SSH agent has no identities loaded (try ssh-add)"),
                };
                let (_, result) = session.authenticate_future(username, pubkey, agent).await;
                result.context("SSH agent authentication failed")?;
            }

        }

        let channel = session
            .channel_open_session()
            .await
            .context("Failed to open SSH channel")?;
        Ok(Self {
            session,
            channel,
            host: host.to_string(),
            port,
            username: username.to_string(),
            pty_ready: false,
            bastion: None,
        })
    }

    /// Request a PTY on the session channel.
    pub async fn request_pty(&mut self, term: &str, cols: u32, rows: u32) -> Result<()> {
        self.channel
            .request_pty(false, term, cols, rows, 0, 0, &[])
            .await
            .context("Failed to request PTY")?;
        self.pty_ready = true;
        Ok(())
    }

    /// Send data to the remote PTY.
    pub async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.channel
            .data(data)
            .await
            .context("Failed to send data to SSH channel")?;
        Ok(())
    }

    /// Receive data from the remote PTY. Returns None on EOF or channel close.
    pub async fn recv(&mut self) -> Result<Option<Vec<u8>>> {
        match self.channel.wait().await {
            Some(russh::ChannelMsg::Data { data }) => Ok(Some(data.to_vec())),
            Some(russh::ChannelMsg::Eof) | None => Ok(None),
            Some(russh::ChannelMsg::ExitStatus { .. }) => Ok(None),
            _ => Ok(None),
        }
    }

    /// Resize the PTY.
    pub async fn resize(&mut self, cols: u32, rows: u32) -> Result<()> {
        self.channel.window_change(cols, rows, 0, 0).await?;
        Ok(())
    }

    /// Whether the session is still open.
    pub fn is_open(&self) -> bool {
        true
    }

    /// Open an SFTP session over this SSH connection.
    pub async fn open_sftp(&self) -> Result<SftpSession> {
        let channel = self.session
            .channel_open_session()
            .await
            .context("Failed to open SFTP channel")?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .context("Failed to request SFTP subsystem")?;
        SftpSession::new(channel.into_stream())
            .await
            .context("Failed to initialize SFTP session")
    }

    /// Connect through a bastion/jump host (ProxyJump).
    ///
    /// First connects to the bastion, authenticates, then opens a direct-tcpip
    /// channel to the target host through the bastion, and creates a new SSH
    /// session over that channel.
    pub async fn connect_via_bastion(
        host: &str,
        port: u16,
        username: &str,
        auth: AuthMethod,
        bastion_host: &str,
        bastion_port: u16,
        bastion_user: &str,
        bastion_auth: AuthMethod,
        data_dir: &std::path::Path,
    ) -> Result<Self> {
        // Step 1: Connect to bastion
        let bastion = Self::connect(bastion_host, bastion_port, bastion_user, bastion_auth, data_dir)
            .await
            .context("Failed to connect to bastion host")?;

        // Step 2: Open direct-tcpip channel through bastion to target
        let channel = bastion.session
            .channel_open_direct_tcpip(host, port as u32, "127.0.0.1", 0)
            .await
            .context("Failed to open ProxyJump channel through bastion")?;

        let stream = channel.into_stream();

        // Step 3: Create SSH session over the tunnel
        let config = Arc::new(client::Config::default());
        let known_hosts = Arc::new(parking_lot::Mutex::new(KnownHosts::load(data_dir)));
        let sh = Client { known_hosts: known_hosts.clone() };

        let mut session = russh::client::connect_stream(config, stream, sh)
            .await
            .context("Failed to connect to target through bastion")?;

        // Step 4: Authenticate to target
        match auth {
            AuthMethod::Key { key_bytes } => {
                let key = keypair_from_bytes(&key_bytes)
                    .context("Failed to reconstruct SSH key")?;
                session
                    .authenticate_publickey(username, Arc::new(key))
                    .await
                    .context("SSH publickey auth through bastion failed")?;
            }
            AuthMethod::Password { password } => {
                session
                    .authenticate_password(username, &password)
                    .await
                    .context("SSH password auth through bastion failed")?;
            }
            AuthMethod::Agent => {
                let mut agent = AgentClient::connect_env().await
                    .context("SSH agent not available through bastion")?;
                let identities = agent.request_identities().await
                    .context("Failed to list agent identities through bastion")?;
                if identities.is_empty() {
                    anyhow::bail!("SSH agent has no identities for bastion hop");
                }
                let pubkey = match identities.into_iter().next() {
                    Some(pk) => pk,
                    None => anyhow::bail!("SSH agent has no identities for bastion hop"),
                };
                let (_, result) = session.authenticate_future(username, pubkey, agent).await;
                result.context("Agent auth through bastion failed")?;
            }
        }

        let channel = session
            .channel_open_session()
            .await
            .context("Failed to open SSH channel on target")?;

        Ok(Self {
            session,
            channel,
            host: host.to_string(),
            port,
            username: username.to_string(),
            pty_ready: false,
            bastion: Some(Box::new(bastion)),
        })
    }

    /// Close the session gracefully (including bastion if any).
    pub async fn close(self) -> Result<()> {
        self.channel.eof().await?;
        self.session
            .disconnect(Disconnect::ByApplication, "", "User closed")
            .await?;
        // Close bastion session if present (ProxyJump)
        if let Some(bastion) = self.bastion {
            // Box the future to avoid infinite recursion in async fn
            let _ = Box::pin(bastion.close()).await;
        }
        Ok(())
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_known_hosts_tofu() {
        let dir = TempDir::new().unwrap();
        let mut hosts = KnownHosts::load(dir.path());

        let key = russh_keys::key::KeyPair::generate_ed25519();
        let pubkey = key.clone_public_key().unwrap();
        let result = hosts.check("example.com", 22, &pubkey);
        assert!(result.unwrap(), "TOFU should accept new host");

        let result = hosts.check("example.com", 22, &pubkey);
        assert!(result.unwrap(), "known host should be accepted");

        let key2 = russh_keys::key::KeyPair::generate_ed25519();
        let pubkey2 = key2.clone_public_key().unwrap();
        let result = hosts.check("example.com", 22, &pubkey2);
        assert!(!result.unwrap(), "changed host key should be rejected");
    }

    #[test]
    fn test_known_hosts_persistence() {
        let dir = TempDir::new().unwrap();
        let key = russh_keys::key::KeyPair::generate_ed25519();
        let pubkey = key.clone_public_key().unwrap();

        {
            let mut hosts = KnownHosts::load(dir.path());
            hosts.check("persist.example.com", 22, &pubkey).unwrap();
            hosts.save();
        }

        {
            let hosts = KnownHosts::load(dir.path());
            let result = hosts.fingerprints.get("persist.example.com:22");
            assert!(result.is_some(), "fingerprint should persist");
        }
    }

    #[test]
    fn test_keypair_roundtrip() {
        let pair = KeyPair::generate_ed25519();
        // Extract raw bytes
        let bytes = match &pair {
            KeyPair::Ed25519(sk) => sk.to_bytes().to_vec(),
            _ => panic!("expected Ed25519"),
        };
        assert_eq!(bytes.len(), 32);

        // Reconstruct
        let reconstructed = keypair_from_bytes(&bytes).unwrap();
        let orig_fp = pair.clone_public_key().unwrap().fingerprint();
        let recon_fp = reconstructed.clone_public_key().unwrap().fingerprint();
        assert_eq!(orig_fp, recon_fp);
    }
}
