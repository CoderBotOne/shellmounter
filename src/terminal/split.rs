#![allow(dead_code)]
//! Terminal split pane — horizontal and vertical terminal splits.
//!
//! Manages a tree of terminal panes that can be split horizontally or vertically.
//! Each pane can host an independent SSH session or a snippet runner.

use serde::{Deserialize, Serialize};

/// A split pane in the terminal area.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TerminalPane {
    /// A single terminal (leaf node)
    Leaf {
        id: String,
        host_id: Option<String>,
        host_label: String,
    },
    /// A horizontal split (side by side)
    Horizontal {
        children: Vec<TerminalPane>,
        /// Split ratios (should sum to 1.0)
        ratios: Vec<f32>,
    },
    /// A vertical split (stacked)
    Vertical {
        children: Vec<TerminalPane>,
        ratios: Vec<f32>,
    },
}

/// Split direction for UI actions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// Manager for terminal pane layout.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalLayout {
    pub root: TerminalPane,
}

impl Default for TerminalLayout {
    fn default() -> Self {
        Self {
            root: TerminalPane::Leaf {
                id: "main".into(),
                host_id: None,
                host_label: "Welcome".into(),
            },
        }
    }
}

impl TerminalPane {
    /// Split this pane in the given direction.
    pub fn split(&mut self, direction: SplitDirection, new_pane: TerminalPane) {
        let old = std::mem::replace(
            self,
            TerminalPane::Leaf {
                id: String::new(),
                host_id: None,
                host_label: String::new(),
            },
        );

        match direction {
            SplitDirection::Horizontal => {
                *self = TerminalPane::Horizontal {
                    children: vec![old, new_pane],
                    ratios: vec![0.5, 0.5],
                };
            }
            SplitDirection::Vertical => {
                *self = TerminalPane::Vertical {
                    children: vec![old, new_pane],
                    ratios: vec![0.5, 0.5],
                };
            }
        }
    }

    /// Count total leaf panes.
    pub fn leaf_count(&self) -> usize {
        match self {
            TerminalPane::Leaf { .. } => 1,
            TerminalPane::Horizontal { children, .. }
            | TerminalPane::Vertical { children, .. } => {
                children.iter().map(|c| c.leaf_count()).sum()
            }
        }
    }

    /// Find a leaf pane by ID.
    pub fn find_mut(&mut self, id: &str) -> Option<&mut TerminalPane> {
        match self {
            TerminalPane::Leaf { id: leaf_id, .. } if leaf_id == id => Some(self),
            TerminalPane::Horizontal { children, .. }
            | TerminalPane::Vertical { children, .. } => {
                for child in children {
                    if let Some(found) = child.find_mut(id) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Resize a split by dragging the separator.
    pub fn resize(&mut self, _index: usize, _delta: f32) {
        // Simplified: evenly redistribute
        match self {
            TerminalPane::Horizontal { ref mut ratios, children }
            | TerminalPane::Vertical { ref mut ratios, children } => {
                let len = children.len() as f32;
                *ratios = vec![1.0 / len; children.len()];
            }
            _ => {}
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_layout() {
        let layout = TerminalLayout::default();
        assert_eq!(layout.root.leaf_count(), 1);
    }

    #[test]
    fn test_horizontal_split() {
        let mut root = TerminalPane::Leaf {
            id: "a".into(),
            host_id: None,
            host_label: "A".into(),
        };

        root.split(
            SplitDirection::Horizontal,
            TerminalPane::Leaf {
                id: "b".into(),
                host_id: None,
                host_label: "B".into(),
            },
        );

        assert_eq!(root.leaf_count(), 2);
        match &root {
            TerminalPane::Horizontal { ratios, .. } => {
                assert_eq!(ratios.len(), 2);
            }
            _ => panic!("Expected Horizontal"),
        }
    }

    #[test]
    fn test_find_pane() {
        let mut root = TerminalPane::Leaf {
            id: "a".into(),
            host_id: None,
            host_label: "A".into(),
        };

        root.split(
            SplitDirection::Vertical,
            TerminalPane::Leaf {
                id: "b".into(),
                host_id: None,
                host_label: "B".into(),
            },
        );

        let found = root.find_mut("b");
        assert!(found.is_some());

        let not_found = root.find_mut("z");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_nested_split() {
        let mut root = TerminalPane::Leaf {
            id: "1".into(),
            host_id: None,
            host_label: "".into(),
        };

        root.split(
            SplitDirection::Horizontal,
            TerminalPane::Leaf {
                id: "2".into(),
                host_id: None,
                host_label: "".into(),
            },
        );

        // Split the second pane vertically
        let pane = root.find_mut("2").unwrap();
        pane.split(
            SplitDirection::Vertical,
            TerminalPane::Leaf {
                id: "3".into(),
                host_id: None,
                host_label: "".into(),
            },
        );

        assert_eq!(root.leaf_count(), 3);
    }

    #[test]
    fn test_deserialize() {
        let json = r#"{"root":{"Leaf":{"id":"main","host_id":null,"host_label":"Welcome"}}}"#;
        let layout: TerminalLayout = serde_json::from_str(json).unwrap();
        assert_eq!(layout.root.leaf_count(), 1);
    }
}
