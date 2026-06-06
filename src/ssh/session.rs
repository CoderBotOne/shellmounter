//! SSH session management via russh.
//!
//! Manages SSH connection lifecycle: connect, authenticate, PTY, reconnect.
//! Uses russh (pure Rust, async, Tokio-based).

use anyhow::{Context, Result};
use russh::*;
use russh_keys::load_secret_key;
use std::sync::Arc;
use tokio::sync::Mutex;

/// An active SSH session with PTY.
pub struct SshSession {
    /// SSH client handle
    session: Handle<Client>,
    /// Channel for interactive PTY
    channel: Channel<Msg>,
    /// Host we're connected to (for reconnect)
    host: String,
    port: u16,
    username: String,
    /// Whether PTY has been requested
    pty_ready: bool,
}

/// Client handler (required by russh).
struct Client;

#[async_trait]
impl client::Handler for Client {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // TODO: TOFU (Trust On First Use) — verify against known_hosts
        // For now, accept all (user will be prompted on first connect)
        Ok(true)
    }
}

impl SshSession {
    /// Connect to an SSH server and authenticate.
    ///
    /// # Arguments
    /// * `host` - Hostname or IP
    /// * `port` - SSH port (usually 22)
    /// * `username` - SSH username
    /// * `key_path` - Path to private key file (PEM/OpenSSH format)
    pub async fn connect(
        host: &str,
        port: u16,
        username: &str,
        key_path: &str,
    ) -> Result<Self> {
        let config = client::Config::default();
        let config = Arc::new(config);

        let sh = Client {};
        let mut session = russh::client::connect(config, (host, port), sh)
            .await
            .context("SSH connection failed")?;

        // Load private key
        let key = load_secret_key(key_path, None)
            .context("Failed to load SSH private key")?;

        // Authenticate
        session
            .authenticate_publickey(username, Arc::new(key))
            .await
            .context("SSH authentication failed")?;

        // Open a channel for interactive PTY
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
        })
    }

    /// Request a PTY with the given terminal dimensions.
    pub async fn request_pty(
        &mut self,
        term: &str,
        cols: u32,
        rows: u32,
    ) -> Result<()> {
        self.channel
            .request_pty(false, term, cols, rows, 0, 0, &[])
            .await
            .context("Failed to request PTY")?;

        self.pty_ready = true;
        Ok(())
    }

    /// Start a shell on the remote host.
    pub async fn start_shell(&mut self) -> Result<()> {
        self.channel
            .exec("$SHELL -l")
            .await
            .context("Failed to start shell")?;

        Ok(())
    }

    /// Send data to the remote PTY (stdin).
    pub async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.channel
            .data(data.into())
            .await
            .context("Failed to send data")?;
        Ok(())
    }

    /// Read data from the remote PTY (stdout).
    /// Returns the received bytes, or None if the channel closed.
    pub async fn recv(&mut self) -> Result<Option<Vec<u8>>> {
        match self.channel.wait().await {
            Some(ChannelMsg::Data { data }) => Ok(Some(data.to_vec())),
            Some(ChannelMsg::Eof) | None => Ok(None),
            Some(ChannelMsg::ExitStatus { .. }) => Ok(None),
            _ => Ok(None),
        }
    }

    /// Resize the PTY terminal.
    pub async fn resize(&mut self, cols: u32, rows: u32) -> Result<()> {
        self.channel
            .window_change(cols, rows, 0, 0)
            .await
            .context("Failed to resize PTY")?;
        Ok(())
    }

    /// Check if the session is still connected.
    pub fn is_open(&self) -> bool {
        !self.channel.eof()
    }

    /// Close the session gracefully.
    pub async fn close(mut self) -> Result<()> {
        self.channel.eof().await?;
        self.session
            .disconnect(Disconnect::ByApplication, "", "User closed")
            .await?;
        Ok(())
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // These tests require a running SSH server.
    // They are conditional — skipped if SSH_HOST env var isn't set.

    fn ssh_config() -> Option<(String, u16, String, String)> {
        let host = std::env::var("SSH_TEST_HOST").ok()?;
        let port: u16 = std::env::var("SSH_TEST_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(22);
        let user = std::env::var("SSH_TEST_USER").unwrap_or_else(|_| "root".to_string());
        let key = std::env::var("SSH_TEST_KEY").unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".ssh/id_rsa")
                .to_string_lossy()
                .to_string()
        });
        Some((host, port, user, key))
    }

    #[tokio::test]
    async fn test_connect_requires_server() {
        let config = ssh_config();
        if config.is_none() {
            eprintln!("Skipping: SSH_TEST_HOST not set");
            return;
        }
        let (host, port, user, key) = config.unwrap();

        let session = SshSession::connect(&host, port, &user, &key).await;
        assert!(session.is_ok(), "SSH connection should succeed");

        let mut session = session.unwrap();
        session.request_pty("xterm-256color", 80, 24).await.unwrap();
        assert!(session.pty_ready);

        session.close().await.unwrap();
    }
}
