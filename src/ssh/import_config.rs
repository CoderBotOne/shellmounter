//! SSH config parser — imports hosts from ~/.ssh/config.
//!
//! Parses the standard OpenSSH client config format:
//!   Host, HostName, User, Port, IdentityFile, ProxyJump, ForwardAgent
//!
//! # Example
//! ```ignore
//! let hosts = parse_ssh_config("~/.ssh/config")?;
//! for host in hosts {
//!     db.upsert_host(&host)?;
//! }
//! ```

use crate::db::hosts::{AuthMethod, Host};
use anyhow::{Context, Result};
use std::path::Path;

/// Parse an OpenSSH config file and return a list of Host entries.
pub fn parse_ssh_config(path: &Path) -> Result<Vec<Host>> {
    let contents = std::fs::read_to_string(path)
        .context("Failed to read SSH config")?;

    let mut hosts = Vec::new();
    let mut current: Option<HostBuilder> = None;

    for line in contents.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split on whitespace: "HostName example.com" → ("HostName", "example.com")
        let mut parts = line.splitn(2, |c: char| c.is_whitespace());
        let keyword = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("").trim();

        match keyword.to_lowercase().as_str() {
            "host" => {
                // Save previous host
                if let Some(builder) = current.take() {
                    if !builder.is_wildcard() {
                        hosts.push(builder.build());
                    }
                }
                // Start new host — value can be multiple patterns: "Host dev prod-*"
                current = Some(HostBuilder::new(value));
            }
            "hostname" => {
                if let Some(ref mut b) = current {
                    b.hostname = value.to_string();
                }
            }
            "user" => {
                if let Some(ref mut b) = current {
                    b.username = value.to_string();
                }
            }
            "port" => {
                if let Some(ref mut b) = current {
                    b.port = value.parse().unwrap_or(22);
                }
            }
            "identityfile" => {
                if let Some(ref mut b) = current {
                    // Expand ~ to home dir
                    let expanded = shellexpand::tilde(value).to_string();
                    b.identity_file = Some(expanded);
                }
            }
            "proxyjump" | "proxycommand" => {
                if let Some(ref mut b) = current {
                    b.bastion = Some(value.to_string());
                }
            }
            "forwardagent" => {
                if let Some(ref mut b) = current {
                    b.forward_agent = value.eq_ignore_ascii_case("yes");
                }
            }
            _ => {
                // Unknown keyword — skip
            }
        }
    }

    // Save last host
    if let Some(builder) = current.take() {
        if !builder.is_wildcard() {
            hosts.push(builder.build());
        }
    }

    Ok(hosts)
}

/// Builder for constructing Host entries from parsed config.
struct HostBuilder {
    label: String,
    hostname: String,
    port: u16,
    username: String,
    identity_file: Option<String>,
    bastion: Option<String>,
    forward_agent: bool,
}

impl HostBuilder {
    fn new(label: &str) -> Self {
        // Take the first pattern as label (e.g., "dev" from "Host dev prod-*")
        let label = label.split_whitespace().next().unwrap_or(label).to_string();
        Self {
            label,
            hostname: String::new(),
            port: 22,
            username: String::new(),
            identity_file: None,
            bastion: None,
            forward_agent: false,
        }
    }

    fn is_wildcard(&self) -> bool {
        self.label == "*" || self.label.starts_with('*')
    }

    fn build(self) -> Host {
        let auth_method = if self.forward_agent {
            AuthMethod::Agent
        } else if let Some(_key_path) = self.identity_file {
            AuthMethod::Key {
                vault_id: format!("imported-{}", sanitize_id(&self.label)),
            }
        } else {
            AuthMethod::Agent
        };

        let id = sanitize_id(&self.label);
        let hostname = if self.hostname.is_empty() {
            self.label.clone()
        } else {
            self.hostname
        };

        Host {
            id,
            label: self.label,
            hostname,
            port: self.port,
            username: if self.username.is_empty() {
                "root".into()
            } else {
                self.username
            },
            auth_method,
            group_name: Some("Imported".into()),
            tags: vec![],
            bastion_id: self.bastion,
            keep_alive_secs: 30,
            created_at: 0,
            updated_at: 0,
        }
    }
}

fn sanitize_id(label: &str) -> String {
    label
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_lowercase()
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_config(content: &str) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        dir
    }

    #[test]
    fn test_parse_simple_host() {
        let config = r#"
Host myserver
    HostName 10.0.1.50
    User admin
    Port 2222
"#;
        let dir = write_config(config);
        let hosts = parse_ssh_config(&dir.path().join("config")).unwrap();

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].label, "myserver");
        assert_eq!(hosts[0].hostname, "10.0.1.50");
        assert_eq!(hosts[0].username, "admin");
        assert_eq!(hosts[0].port, 2222);
    }

    #[test]
    fn test_parse_multiple_hosts() {
        let config = r#"
Host prod-web
    HostName web.prod.example.com
    User deploy

Host prod-db
    HostName 10.0.2.100
    User postgres
    Port 5432
"#;
        let dir = write_config(config);
        let hosts = parse_ssh_config(&dir.path().join("config")).unwrap();

        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].label, "prod-web");
        assert_eq!(hosts[1].label, "prod-db");
        assert_eq!(hosts[1].port, 5432);
    }

    #[test]
    fn test_skip_wildcard() {
        let config = r#"
Host *
    ForwardAgent yes
    User default-user

Host specific
    HostName specific.example.com
"#;
        let dir = write_config(config);
        let hosts = parse_ssh_config(&dir.path().join("config")).unwrap();

        // Wildcard * should be skipped, only "specific" should remain
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].label, "specific");
    }

    #[test]
    fn test_parse_identity_file() {
        let config = r#"
Host withkey
    HostName example.com
    IdentityFile ~/.ssh/id_ed25519
"#;
        let dir = write_config(config);
        let hosts = parse_ssh_config(&dir.path().join("config")).unwrap();

        assert_eq!(hosts.len(), 1);
        match &hosts[0].auth_method {
            AuthMethod::Key { vault_id } => {
                assert!(vault_id.contains("withkey"));
            }
            _ => panic!("Expected AuthMethod::Key"),
        }
    }

    #[test]
    fn test_comments_and_empty_lines() {
        let config = r#"
# This is a comment

Host server
    # Inline comment should be ignored
    HostName server.example.com
    User root

# Another comment
"#;
        let dir = write_config(config);
        let hosts = parse_ssh_config(&dir.path().join("config")).unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].label, "server");
    }

    #[test]
    fn test_empty_config() {
        let dir = write_config("");
        let hosts = parse_ssh_config(&dir.path().join("config")).unwrap();
        assert!(hosts.is_empty());
    }
}
