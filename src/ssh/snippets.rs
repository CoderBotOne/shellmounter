//! Snippet library — store and execute frequently-used commands.
//!
//! Snippets are stored in SQLite and can be quickly sent to the active terminal
//! via Ctrl+Shift+S or by clicking in the snippet panel.
//!
//! Memory: Vec<Snippet> loaded once, diffed on add/delete. No allocations per frame.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// A saved command snippet.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub label: String,
    pub command: String,
    pub description: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Snippet {
    /// Resolve template variables in the command string.
    ///
    /// Supported placeholders:
    ///   `${host}`   — remote hostname
    ///   `${user}`   — remote username
    ///   `${port}`   — remote port
    ///   `${label}`  — host label
    ///
    /// If `host` is `None`, placeholders are left as-is.
    pub fn resolve(&self, host: Option<(&str, &str, u16, &str)>) -> String {
        let mut cmd = self.command.clone();
        if let Some((hostname, username, port, label)) = host {
            cmd = cmd.replace("${host}", hostname);
            cmd = cmd.replace("${user}", username);
            cmd = cmd.replace("${port}", &port.to_string());
            cmd = cmd.replace("${label}", label);
        }
        cmd
    }
}

/// Snippet store backed by SQLite.
pub struct SnippetStore {
    conn: Connection,
}

impl SnippetStore {
    /// Open the snippet database (creates table if needed).
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS snippets (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                command TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_snippets_label ON snippets(label);",
        )?;

        Ok(Self { conn })
    }

    /// Add or update a snippet.
    pub fn save(&self, snippet: &Snippet) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let tags_json = serde_json::to_string(&snippet.tags)?;

        self.conn.execute(
            "INSERT INTO snippets (id, label, command, description, tags, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                label = excluded.label,
                command = excluded.command,
                description = excluded.description,
                tags = excluded.tags,
                updated_at = excluded.updated_at",
            params![
                snippet.id,
                snippet.label,
                snippet.command,
                snippet.description,
                tags_json,
                now,
                now,
            ],
        )?;

        Ok(())
    }

    /// List all snippets, ordered by label.
    pub fn list(&self) -> Result<Vec<Snippet>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, command, description, tags, created_at, updated_at
             FROM snippets ORDER BY label",
        )?;

        let rows = stmt.query_map([], |row| {
            let tags_str: String = row.get(4)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(Snippet {
                id: row.get(0)?,
                label: row.get(1)?,
                command: row.get(2)?,
                description: row.get(3)?,
                tags,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;

        let mut snippets = Vec::new();
        for row in rows {
            snippets.push(row?);
        }
        Ok(snippets)
    }

    /// Search snippets by label or command text.
    pub fn search(&self, query: &str) -> Result<Vec<Snippet>> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT id, label, command, description, tags, created_at, updated_at
             FROM snippets
             WHERE label LIKE ?1 OR command LIKE ?1
             ORDER BY label",
        )?;

        let rows = stmt.query_map(params![pattern], |row| {
            let tags_str: String = row.get(4)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(Snippet {
                id: row.get(0)?,
                label: row.get(1)?,
                command: row.get(2)?,
                description: row.get(3)?,
                tags,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;

        let mut snippets = Vec::new();
        for row in rows {
            snippets.push(row?);
        }
        Ok(snippets)
    }

    /// Delete a snippet.
    pub fn delete(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM snippets WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Get a single snippet by ID.
    pub fn get(&self, id: &str) -> Result<Option<Snippet>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, command, description, tags, created_at, updated_at
             FROM snippets WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            let tags_str: String = row.get(4)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(Snippet {
                id: row.get(0)?,
                label: row.get(1)?,
                command: row.get(2)?,
                description: row.get(3)?,
                tags,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;

        match rows.next() {
            Some(Ok(s)) => Ok(Some(s)),
            _ => Ok(None),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn test_save_and_list() {
        let dir = TempDir::new().unwrap();
        let store = SnippetStore::open(&dir.path().join("snippets.db")).unwrap();

        let s = Snippet {
            id: Uuid::new_v4().to_string(),
            label: "Restart nginx".into(),
            command: "sudo systemctl restart nginx".into(),
            description: "Restart nginx web server".into(),
            tags: vec!["web".into(), "nginx".into()],
            created_at: 0,
            updated_at: 0,
        };

        store.save(&s).unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].command, "sudo systemctl restart nginx");
    }

    #[test]
    fn test_search() {
        let dir = TempDir::new().unwrap();
        let store = SnippetStore::open(&dir.path().join("snippets.db")).unwrap();

        let s1 = Snippet {
            id: Uuid::new_v4().to_string(),
            label: "DB backup".into(),
            command: "pg_dump -U postgres mydb".into(),
            description: "".into(),
            tags: vec![],
            created_at: 0,
            updated_at: 0,
        };
        let s2 = Snippet {
            id: Uuid::new_v4().to_string(),
            label: "Deploy app".into(),
            command: "ansible-playbook deploy.yml".into(),
            description: "".into(),
            tags: vec![],
            created_at: 0,
            updated_at: 0,
        };

        store.save(&s1).unwrap();
        store.save(&s2).unwrap();

        let results = store.search("postgres").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "DB backup");

        let results = store.search("ansible").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "Deploy app");
    }

    #[test]
    fn test_delete() {
        let dir = TempDir::new().unwrap();
        let store = SnippetStore::open(&dir.path().join("snippets.db")).unwrap();

        let s = Snippet {
            id: "test-id".into(),
            label: "Test".into(),
            command: "echo test".into(),
            description: "".into(),
            tags: vec![],
            created_at: 0,
            updated_at: 0,
        };

        store.save(&s).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);

        store.delete("test-id").unwrap();
        assert_eq!(store.list().unwrap().len(), 0);
    }
}
