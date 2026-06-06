//! Host database — CRUD for SSH hosts, groups, and tags.
//!
//! Uses SQLite via rusqlite (bundled, no system dependency).
//! Schema is auto-migrated on first open.

use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// An SSH host configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    pub label: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub auth_method: AuthMethod,
    pub group_name: Option<String>,
    pub tags: Vec<String>,
    pub bastion_id: Option<String>,
    pub keep_alive_secs: u32,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AuthMethod {
    /// SSH key stored in vault (vault_id)
    Key { vault_id: String },
    /// Password stored in vault (vault_id)
    Password { vault_id: String },
    /// SSH agent forwarding
    Agent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostGroup {
    pub name: String,
    pub collapsed: bool,
    pub sort_order: i32,
}

pub struct HostDb {
    conn: Connection,
}

impl HostDb {
    /// Open (and auto-migrate) the host database.
    pub fn open(data_dir: &Path) -> SqlResult<Self> {
        let db_path = data_dir.join("hosts.db");
        let conn = Connection::open(&db_path)?;

        // Enable WAL mode for better concurrent reads
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> SqlResult<()> {
        let version: i32 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);

        if version < 1 {
            self.conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS hosts (
                    id TEXT PRIMARY KEY,
                    label TEXT NOT NULL,
                    hostname TEXT NOT NULL,
                    port INTEGER NOT NULL DEFAULT 22,
                    username TEXT NOT NULL DEFAULT 'root',
                    auth_method TEXT NOT NULL DEFAULT 'agent',
                    vault_id TEXT,
                    group_name TEXT,
                    bastion_id TEXT,
                    keep_alive_secs INTEGER NOT NULL DEFAULT 30,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS host_tags (
                    host_id TEXT NOT NULL,
                    tag TEXT NOT NULL,
                    PRIMARY KEY (host_id, tag),
                    FOREIGN KEY (host_id) REFERENCES hosts(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS groups (
                    name TEXT PRIMARY KEY,
                    collapsed INTEGER NOT NULL DEFAULT 0,
                    sort_order INTEGER NOT NULL DEFAULT 0
                );

                PRAGMA user_version = 1;",
            )?;
        }

        Ok(())
    }

    /// Insert or update a host.
    pub fn upsert_host(&self, host: &Host) -> SqlResult<()> {
        let auth_method = match &host.auth_method {
            AuthMethod::Key { vault_id } | AuthMethod::Password { vault_id } => {
                (host.auth_method_name(), Some(vault_id.clone()))
            }
            AuthMethod::Agent => ("agent".to_string(), None),
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT INTO hosts (id, label, hostname, port, username, auth_method, vault_id, 
             group_name, bastion_id, keep_alive_secs, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
                label = excluded.label,
                hostname = excluded.hostname,
                port = excluded.port,
                username = excluded.username,
                auth_method = excluded.auth_method,
                vault_id = excluded.vault_id,
                group_name = excluded.group_name,
                bastion_id = excluded.bastion_id,
                keep_alive_secs = excluded.keep_alive_secs,
                updated_at = excluded.updated_at",
            params![
                host.id,
                host.label,
                host.hostname,
                host.port,
                host.username,
                auth_method.0,
                auth_method.1,
                host.group_name,
                host.bastion_id,
                host.keep_alive_secs,
                now,
                now,
            ],
        )?;

        // Sync tags
        self.conn
            .execute("DELETE FROM host_tags WHERE host_id = ?1", params![host.id])?;
        for tag in &host.tags {
            self.conn.execute(
                "INSERT INTO host_tags (host_id, tag) VALUES (?1, ?2)",
                params![host.id, tag],
            )?;
        }

        Ok(())
    }

    /// Get a single host by ID.
    pub fn get_host(&self, id: &str) -> SqlResult<Option<Host>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, hostname, port, username, auth_method, vault_id,
             group_name, bastion_id, keep_alive_secs, created_at, updated_at
             FROM hosts WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            Ok(HostRow {
                id: row.get(0)?,
                label: row.get(1)?,
                hostname: row.get(2)?,
                port: row.get(3)?,
                username: row.get(4)?,
                auth_method: row.get(5)?,
                vault_id: row.get(6)?,
                group_name: row.get(7)?,
                bastion_id: row.get(8)?,
                keep_alive_secs: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?;

        if let Some(row) = rows.next() {
            let row = row?;
            let tags = self.get_tags(&row.id)?;
            Ok(Some(row_to_host(row, tags)))
        } else {
            Ok(None)
        }
    }

    /// List all hosts, optionally filtered by group.
    pub fn list_hosts(&self, group: Option<&str>) -> SqlResult<Vec<Host>> {
        let query = if group.is_some() {
            "SELECT id, label, hostname, port, username, auth_method, vault_id,
             group_name, bastion_id, keep_alive_secs, created_at, updated_at
             FROM hosts WHERE group_name = ?1 ORDER BY label"
        } else {
            "SELECT id, label, hostname, port, username, auth_method, vault_id,
             group_name, bastion_id, keep_alive_secs, created_at, updated_at
             FROM hosts ORDER BY group_name, label"
        };

        let mut stmt = self.conn.prepare(query)?;

        let rows = if let Some(g) = group {
            stmt.query_map(params![g], map_host_row)?
        } else {
            stmt.query_map([], map_host_row)?
        };

        let mut hosts = Vec::new();
        for row in rows {
            let row = row?;
            let tags = self.get_tags(&row.id)?;
            hosts.push(row_to_host(row, tags));
        }

        Ok(hosts)
    }

    /// Delete a host and its tags.
    pub fn delete_host(&self, id: &str) -> SqlResult<()> {
        self.conn
            .execute("DELETE FROM hosts WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Upsert a group.
    pub fn upsert_group(&self, group: &HostGroup) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO groups (name, collapsed, sort_order) VALUES (?1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET collapsed = excluded.collapsed, sort_order = excluded.sort_order",
            params![group.name, group.collapsed as i32, group.sort_order],
        )?;
        Ok(())
    }

    /// List all groups.
    pub fn list_groups(&self) -> SqlResult<Vec<HostGroup>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, collapsed, sort_order FROM groups ORDER BY sort_order")?;
        let rows = stmt.query_map([], |row| {
            Ok(HostGroup {
                name: row.get(0)?,
                collapsed: row.get::<_, i32>(1)? != 0,
                sort_order: row.get(2)?,
            })
        })?;

        let mut groups = Vec::new();
        for row in rows {
            groups.push(row?);
        }
        Ok(groups)
    }

    /// Search hosts by label or hostname.
    pub fn search(&self, query: &str) -> SqlResult<Vec<Host>> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT id, label, hostname, port, username, auth_method, vault_id,
             group_name, bastion_id, keep_alive_secs, created_at, updated_at
             FROM hosts WHERE label LIKE ?1 OR hostname LIKE ?1 ORDER BY label",
        )?;

        let rows = stmt.query_map(params![pattern], map_host_row)?;
        let mut hosts = Vec::new();
        for row in rows {
            let row = row?;
            let tags = self.get_tags(&row.id)?;
            hosts.push(row_to_host(row, tags));
        }
        Ok(hosts)
    }

    // ── Private helpers ────────────────────────────────────────────────

    fn get_tags(&self, host_id: &str) -> SqlResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT tag FROM host_tags WHERE host_id = ?1")?;
        let tags = stmt
            .query_map(params![host_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }
}

impl Host {
    fn auth_method_name(&self) -> String {
        match &self.auth_method {
            AuthMethod::Key { .. } => "key".to_string(),
            AuthMethod::Password { .. } => "password".to_string(),
            AuthMethod::Agent => "agent".to_string(),
        }
    }
}

// ── Row mapping ────────────────────────────────────────────────────────

struct HostRow {
    id: String,
    label: String,
    hostname: String,
    port: u16,
    username: String,
    auth_method: String,
    vault_id: Option<String>,
    group_name: Option<String>,
    bastion_id: Option<String>,
    keep_alive_secs: u32,
    created_at: i64,
    updated_at: i64,
}

fn map_host_row(row: &rusqlite::Row) -> SqlResult<HostRow> {
    Ok(HostRow {
        id: row.get(0)?,
        label: row.get(1)?,
        hostname: row.get(2)?,
        port: row.get(3)?,
        username: row.get(4)?,
        auth_method: row.get(5)?,
        vault_id: row.get(6)?,
        group_name: row.get(7)?,
        bastion_id: row.get(8)?,
        keep_alive_secs: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn row_to_host(row: HostRow, tags: Vec<String>) -> Host {
    let auth_method = match (row.auth_method.as_str(), row.vault_id) {
        ("key", Some(vault_id)) => AuthMethod::Key { vault_id },
        ("password", Some(vault_id)) => AuthMethod::Password { vault_id },
        _ => AuthMethod::Agent,
    };

    Host {
        id: row.id,
        label: row.label,
        hostname: row.hostname,
        port: row.port,
        username: row.username,
        auth_method,
        group_name: row.group_name,
        tags,
        bastion_id: row.bastion_id,
        keep_alive_secs: row.keep_alive_secs,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (HostDb, TempDir) {
        let dir = TempDir::new().expect("tempdir");
        let db = HostDb::open(dir.path()).expect("open db");
        (db, dir)
    }

    fn make_host(id: &str, name: &str) -> Host {
        Host {
            id: id.to_string(),
            label: name.to_string(),
            hostname: format!("{}.example.com", id),
            port: 22,
            username: "root".to_string(),
            auth_method: AuthMethod::Agent,
            group_name: None,
            tags: vec![],
            bastion_id: None,
            keep_alive_secs: 30,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn test_upsert_and_get() {
        let (db, _dir) = setup();
        let host = make_host("web-01", "Web Server 1");
        db.upsert_host(&host).expect("upsert");

        let got = db.get_host("web-01").expect("get").expect("exists");
        assert_eq!(got.label, "Web Server 1");
        assert_eq!(got.hostname, "web-01.example.com");
    }

    #[test]
    fn test_update_existing() {
        let (db, _dir) = setup();
        let mut host = make_host("db-01", "DB Server");
        db.upsert_host(&host).expect("insert");

        host.label = "DB Server Updated".to_string();
        host.port = 2222;
        db.upsert_host(&host).expect("update");

        let got = db.get_host("db-01").expect("get").expect("exists");
        assert_eq!(got.label, "DB Server Updated");
        assert_eq!(got.port, 2222);
    }

    #[test]
    fn test_delete() {
        let (db, _dir) = setup();
        db.upsert_host(&make_host("x", "X")).expect("insert");
        assert!(db.get_host("x").expect("get").is_some());

        db.delete_host("x").expect("delete");
        assert!(db.get_host("x").expect("get").is_none());
    }

    #[test]
    fn test_list_all() {
        let (db, _dir) = setup();
        db.upsert_host(&make_host("a", "A")).expect("insert");
        db.upsert_host(&make_host("b", "B")).expect("insert");

        let hosts = db.list_hosts(None).expect("list");
        assert_eq!(hosts.len(), 2);
    }

    #[test]
    fn test_list_by_group() {
        let (db, _dir) = setup();
        let mut h1 = make_host("h1", "H1");
        h1.group_name = Some("Production".to_string());
        let mut h2 = make_host("h2", "H2");
        h2.group_name = Some("Staging".to_string());

        db.upsert_host(&h1).expect("insert");
        db.upsert_host(&h2).expect("insert");

        let prod = db.list_hosts(Some("Production")).expect("list");
        assert_eq!(prod.len(), 1);
        assert_eq!(prod[0].label, "H1");
    }

    #[test]
    fn test_tags() {
        let (db, _dir) = setup();
        let mut host = make_host("tagged", "Tagged");
        host.tags = vec!["docker".to_string(), "production".to_string()];
        db.upsert_host(&host).expect("insert");

        let got = db.get_host("tagged").expect("get").expect("exists");
        assert_eq!(got.tags.len(), 2);
        assert!(got.tags.contains(&"docker".to_string()));
    }

    #[test]
    fn test_search() {
        let (db, _dir) = setup();
        let mut h1 = make_host("web-prod", "Production Web");
        h1.hostname = "web.prod.example.com".to_string();
        db.upsert_host(&h1).expect("insert");

        let mut h2 = make_host("db-prod", "Production DB");
        h2.hostname = "db.prod.example.com".to_string();
        db.upsert_host(&h2).expect("insert");

        let results = db.search("web").expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "web-prod");

        let results = db.search("prod.example").expect("search");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_groups_crud() {
        let (db, _dir) = setup();
        db.upsert_group(&HostGroup {
            name: "Production".to_string(),
            collapsed: false,
            sort_order: 0,
        })
        .expect("insert");

        let groups = db.list_groups().expect("list");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Production");
    }

    #[test]
    fn test_persistence() {
        let dir = TempDir::new().expect("tempdir");

        {
            let db = HostDb::open(dir.path()).expect("open");
            db.upsert_host(&make_host("persist", "Persistent")).expect("insert");
        }

        {
            let db = HostDb::open(dir.path()).expect("reopen");
            let host = db.get_host("persist").expect("get").expect("exists");
            assert_eq!(host.label, "Persistent");
        }
    }
}
