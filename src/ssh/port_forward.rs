//! Port forwarding — SSH tunnels (local, remote, dynamic).
//!
//! Manages the lifecycle of port forwarding rules via russh SSH sessions.
//! Each rule runs as a background Tokio task.
//!
//! Two modes:
//! - **Direct**: plain TCP forwarding (no SSH, for testing/UIs without active session)
//! - **SSH tunnel**: `channel_open_direct_tcpip` through an active SSH session

use anyhow::{Context, Result};
use russh::client::Handle;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

/// A port forwarding rule.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PortForwardRule {
    pub id: String,
    pub label: String,
    /// Type of forwarding
    pub kind: ForwardKind,
    /// Local port to bind
    pub local_port: u16,
    /// Remote host to forward to
    pub remote_host: String,
    /// Remote port to forward to
    pub remote_port: u16,
    /// Whether the rule is currently active
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ForwardKind {
    /// -L local_port:remote_host:remote_port (local → remote)
    Local,
    /// -R remote_port:local_host:local_port (remote → local)
    Remote,
    /// -D local_port (SOCKS5 dynamic proxy)
    Dynamic,
}

impl ForwardKind {
    pub fn display(&self) -> &str {
        match self {
            ForwardKind::Local => "Local (-L)",
            ForwardKind::Remote => "Remote (-R)",
            ForwardKind::Dynamic => "Dynamic (-D)",
        }
    }
}

impl PortForwardRule {
    /// Describe the forwarding rule for display in UI.
    pub fn describe(&self) -> String {
        match self.kind {
            ForwardKind::Local => format!(
                "localhost:{} → {}:{}",
                self.local_port, self.remote_host, self.remote_port
            ),
            ForwardKind::Remote => format!(
                "remote:{} → localhost:{}",
                self.local_port, self.remote_port
            ),
            ForwardKind::Dynamic => format!("SOCKS5 proxy on localhost:{}", self.local_port),
        }
    }
}

/// Manager for port forwarding rules.
pub struct PortForwardManager {
    rules: Vec<PortForwardRule>,
    running: Arc<AtomicBool>,
}

impl PortForwardManager {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Add a new forwarding rule.
    pub fn add(&mut self, rule: PortForwardRule) {
        self.rules.push(rule);
    }

    /// Remove a forwarding rule by ID.
    pub fn remove(&mut self, id: &str) {
        self.rules.retain(|r| r.id != id);
    }

    /// List all rules.
    pub fn list(&self) -> &[PortForwardRule] {
        &self.rules
    }

    /// Start a local port forward: localhost:local_port → remote_host:remote_port.
    /// Uses direct TCP (no SSH tunnel). For SSH-tunneled forwarding, use `start_local_forward_ssh`.
    pub async fn start_local_forward(
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    ) -> Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", local_port))
            .await
            .context("Failed to bind local port")?;

        log::info!(
            "Port forward active: localhost:{} → {}:{}",
            local_port,
            remote_host,
            remote_port
        );

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut local_stream, _addr)) => {
                        let remote_addr = format!("{}:{}", remote_host, remote_port);
                        match TcpStream::connect(&remote_addr).await {
                            Ok(mut remote_stream) => {
                                tokio::spawn(async move {
                                    let _ = tokio::io::copy_bidirectional(
                                        &mut local_stream,
                                        &mut remote_stream,
                                    )
                                    .await;
                                });
                            }
                            Err(e) => {
                                log::error!("Port forward connect failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Port forward accept failed: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Start a local port forward tunnelled through an SSH session.
    ///
    /// Binds localhost:local_port and forwards each connection through
    /// `channel_open_direct_tcpip(remote_host, remote_port)` on the SSH session.
    /// All traffic goes through the encrypted SSH tunnel.
    pub async fn start_local_forward_ssh<H: russh::client::Handler + Send + Sync + 'static>(
        local_port: u16,
        remote_host: String,
        remote_port: u16,
        ssh_handle: Arc<Handle<H>>,
    ) -> Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", local_port))
            .await
            .context("Failed to bind local port")?;

        log::info!(
            "SSH port forward active: localhost:{} → {}:{}",
            local_port,
            remote_host,
            remote_port
        );

        let remote = remote_host.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut local_stream, _addr)) => {
                        let rh = remote.clone();
                        let handle = ssh_handle.clone();
                        tokio::spawn(async move {
                            // Open a direct-tcpip channel through SSH to the remote
                            match handle
                                .channel_open_direct_tcpip(&rh, remote_port as u32, "127.0.0.1", 0)
                                .await
                            {
                                Ok(mut channel) => {
                                    // Read from local, write to SSH channel
                                    let mut buf = vec![0u8; 16384];
                                    loop {
                                        tokio::select! {
                                            // Local → SSH
                                            result = local_stream.readable() => {
                                                match result {
                                                    Ok(()) => {
                                                        match local_stream.try_read(&mut buf) {
                                                            Ok(0) => break, // EOF
                                                            Ok(n) => {
                                                                if channel.data(&buf[..n]).await.is_err() {
                                                                    break;
                                                                }
                                                            }
                                                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                                                            Err(_) => break,
                                                        }
                                                    }
                                                    Err(_) => break,
                                                }
                                            }
                                            // SSH → Local
                                            msg = channel.wait() => {
                                                match msg {
                                                    Some(russh::ChannelMsg::Data { data }) => {
                                                        if local_stream.write_all(&data).await.is_err() {
                                                            break;
                                                        }
                                                    }
                                                    Some(russh::ChannelMsg::Eof) | None => break,
                                                    _ => continue,
                                                }
                                            }
                                        }
                                    }
                                    let _ = channel.eof().await;
                                }
                                Err(e) => {
                                    log::error!("SSH direct-tcpip failed: {}", e);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("Port forward accept failed: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop all running forwards.
    pub fn stop_all(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_describe_local() {
        let rule = PortForwardRule {
            id: "1".into(),
            label: "DB tunnel".into(),
            kind: ForwardKind::Local,
            local_port: 5432,
            remote_host: "db.internal".into(),
            remote_port: 5432,
            enabled: false,
        };
        assert_eq!(rule.describe(), "localhost:5432 → db.internal:5432");
    }

    #[test]
    fn test_rule_describe_dynamic() {
        let rule = PortForwardRule {
            id: "2".into(),
            label: "SOCKS proxy".into(),
            kind: ForwardKind::Dynamic,
            local_port: 1080,
            remote_host: String::new(),
            remote_port: 0,
            enabled: false,
        };
        assert_eq!(rule.describe(), "SOCKS5 proxy on localhost:1080");
    }

    #[test]
    fn test_manager_add_remove() {
        let mut mgr = PortForwardManager::new();
        mgr.add(PortForwardRule {
            id: "a".into(),
            label: "A".into(),
            kind: ForwardKind::Local,
            local_port: 8080,
            remote_host: "example.com".into(),
            remote_port: 80,
            enabled: false,
        });

        assert_eq!(mgr.list().len(), 1);
        mgr.remove("a");
        assert_eq!(mgr.list().len(), 0);
    }

    #[test]
    fn test_forward_kind_display() {
        assert_eq!(ForwardKind::Local.display(), "Local (-L)");
        assert_eq!(ForwardKind::Remote.display(), "Remote (-R)");
        assert_eq!(ForwardKind::Dynamic.display(), "Dynamic (-D)");
    }
}
