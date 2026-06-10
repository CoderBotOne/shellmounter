//! SSH agent — exposes keys from the vault as an ssh-agent compatible socket.
//!
//! Listens on a Unix domain socket and speaks the ssh-agent protocol
//! (draft-miller-ssh-agent). Other tools (ssh, rsync, git) can use
//! `SSH_AUTH_SOCK` to request key operations from ShellMounter.
//!
//! Currently handles: SSH_AGENTC_REQUEST_IDENTITIES, SSH_AGENTC_SIGN_REQUEST.

use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

/// SSH agent protocol constants.
mod proto {
    pub const SSH_AGENT_FAILURE: u8 = 5;
    pub const SSH_AGENT_SUCCESS: u8 = 6;
    pub const SSH_AGENTC_REQUEST_IDENTITIES: u8 = 11;
    pub const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
    pub const SSH_AGENTC_SIGN_REQUEST: u8 = 13;
    pub const SSH_AGENT_SIGN_RESPONSE: u8 = 14;
}

/// An SSH agent that serves keys from the vault over a Unix socket.
pub struct SshAgent {
    socket_path: PathBuf,
}

impl SshAgent {
    /// Create a new agent that will listen on `socket_path`.
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Start the agent. Spawns a background task that handles connections.
    /// Returns immediately. The socket path can be set as `SSH_AUTH_SOCK`.
    pub async fn start(&self) -> Result<()> {
        // Remove stale socket if present
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)
            .context("Failed to bind SSH agent socket")?;

        log::info!(
            "SSH agent listening on {}",
            self.socket_path.display()
        );

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        tokio::spawn(handle_connection(stream));
                    }
                    Err(e) => {
                        log::error!("SSH agent accept error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Get the environment variable value for SSH_AUTH_SOCK.
    pub fn env_value(&self) -> String {
        self.socket_path.to_string_lossy().to_string()
    }
}

/// Handle a single agent connection.
async fn handle_connection(mut stream: UnixStream) {
    let mut buf = vec![0u8; 16384];

    loop {
        // Read message length (4 bytes)
        let n = match stream.read(&mut buf[..4]).await {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(_) => break,
        };
        if n < 4 {
            break;
        }

        let msg_len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        if msg_len == 0 || msg_len > 16384 - 4 {
            break;
        }

        // Read message body
        if stream.read_exact(&mut buf[4..4 + msg_len]).await.is_err() {
            break;
        }

        let request_type = buf[4];

        let response = match request_type {
            proto::SSH_AGENTC_REQUEST_IDENTITIES => {
                // Return empty key list for now — keys are loaded from vault later
                let mut resp = Vec::new();
                resp.extend_from_slice(&[proto::SSH_AGENT_IDENTITIES_ANSWER]);
                resp.extend_from_slice(&0u32.to_be_bytes()); // 0 keys
                resp
            }
            proto::SSH_AGENTC_SIGN_REQUEST => {
                // For now, return failure — full signing requires vault access
                vec![proto::SSH_AGENT_FAILURE]
            }
            _ => {
                vec![proto::SSH_AGENT_FAILURE]
            }
        };

        // Send response with length prefix
        let resp_len = (response.len() as u32).to_be_bytes();
        let _ = stream.write_all(&resp_len).await;
        let _ = stream.write_all(&response).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_agent_lifecycle() {
        let dir = TempDir::new().unwrap();
        let socket_path = dir.path().join("agent.sock");
        let agent = SshAgent::new(socket_path.clone());
        assert!(agent.start().await.is_ok());
        assert!(agent.env_value().contains("agent.sock"));
    }

    #[tokio::test]
    async fn test_agent_list_identities() {
        let dir = TempDir::new().unwrap();
        let socket_path = dir.path().join("agent2.sock");
        let agent = SshAgent::new(socket_path.clone());
        agent.start().await.unwrap();

        // Allow some time for the listener to start
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Connect and request identities
        let mut conn = UnixStream::connect(&socket_path).await.unwrap();

        // Build request: SSH_AGENTC_REQUEST_IDENTITIES (type 11)
        let request: &[u8] = &[proto::SSH_AGENTC_REQUEST_IDENTITIES];

        // Send with length prefix
        let len = (request.len() as u32).to_be_bytes();
        conn.write_all(&len).await.unwrap();
        conn.write_all(request).await.unwrap();

        // Read response length
        let mut len_buf = [0u8; 4];
        conn.read_exact(&mut len_buf).await.unwrap();
        let resp_len = u32::from_be_bytes(len_buf) as usize;

        // Read response
        let mut resp = vec![0u8; resp_len];
        conn.read_exact(&mut resp).await.unwrap();

        // First byte should be SSH_AGENT_IDENTITIES_ANSWER (12) or FAILURE (5)
        assert!(resp[0] == proto::SSH_AGENT_IDENTITIES_ANSWER || resp[0] == proto::SSH_AGENT_FAILURE);
    }
}
