use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    pub name: String,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub id: String,
    pub name: String,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
    pub assignee: Option<String>, // "ai:agent_name" or "user:name"
    pub priority: Priority,
    pub created_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority { Low, Medium, High, Critical }

impl Priority {
    pub fn label(&self) -> &'static str {
        match self { Priority::Low=>"Low", Priority::Medium=>"Med", Priority::High=>"High", Priority::Critical=>"!!" }
    }
    pub fn color(&self) -> u32 {
        match self { Priority::Low=>0x6b7280, Priority::Medium=>0x3b82f6, Priority::High=>0xf59e0b, Priority::Critical=>0xef4444 }
    }
}

impl Board {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            columns: vec![
                Column::new("col-1", "To Do"),
                Column::new("col-2", "In Progress"),
                Column::new("col-3", "Done"),
            ],
        }
    }

    pub fn load(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn add_card(&mut self, column_id: &str, title: &str) {
        if let Some(col) = self.columns.iter_mut().find(|c| c.id == column_id) {
            col.cards.push(Card {
                id: uuid::Uuid::new_v4().to_string(),
                title: title.to_string(),
                description: String::new(),
                labels: Vec::new(),
                assignee: None,
                priority: Priority::Medium,
                created_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            });
        }
    }

    pub fn move_card(&mut self, card_id: &str, to_column_id: &str) {
        let mut card = None;
        for col in self.columns.iter_mut() {
            if let Some(pos) = col.cards.iter().position(|c| c.id == card_id) {
                card = Some(col.cards.remove(pos));
                break;
            }
        }
        if let Some(card) = card {
            if let Some(col) = self.columns.iter_mut().find(|c| c.id == to_column_id) {
                col.cards.push(card);
            }
        }
    }

    pub fn delete_card(&mut self, card_id: &str) {
        for col in self.columns.iter_mut() {
            col.cards.retain(|c| c.id != card_id);
        }
    }
}

impl Column {
    pub fn new(id: &str, name: &str) -> Self {
        Self { id: id.to_string(), name: name.to_string(), cards: Vec::new() }
    }
}
