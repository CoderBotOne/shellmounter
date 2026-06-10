#![allow(dead_code)]
//! Terminal themes.
//!
//! Loads theme files from ~/.shellmounter/themes/.
//! Built-in themes: Catppuccin Mocha, Dracula, One Dark, Solarized Dark.

use serde::{Deserialize, Serialize};

/// A terminal color theme (16 ANSI colors + foreground/background/cursor).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalTheme {
    pub name: String,
    pub author: String,
    /// Normal colors (0-7): black, red, green, yellow, blue, magenta, cyan, white
    pub colors: [String; 16],
    pub foreground: String,
    pub background: String,
    pub cursor: String,
    pub selection: String,
}

impl Default for TerminalTheme {
    fn default() -> Self {
        Self {
            name: "Catppuccin Mocha".into(),
            author: "Catppuccin".into(),
            colors: [
                "#45475a".into(), // black
                "#f38ba8".into(), // red
                "#a6e3a1".into(), // green
                "#f9e2af".into(), // yellow
                "#89b4fa".into(), // blue
                "#cba6f7".into(), // magenta
                "#94e2d5".into(), // cyan
                "#bac2de".into(), // white
                "#585b70".into(), // bright black
                "#f38ba8".into(), // bright red
                "#a6e3a1".into(), // bright green
                "#f9e2af".into(), // bright yellow
                "#89b4fa".into(), // bright blue
                "#cba6f7".into(), // bright magenta
                "#94e2d5".into(), // bright cyan
                "#a6adc8".into(), // bright white
            ],
            foreground: "#cdd6f4".into(),
            background: "#1e1e2e".into(),
            cursor: "#f5e0dc".into(),
            selection: "#585b70".into(),
        }
    }
}

impl TerminalTheme {
    /// Parse a hex color like "#a6e3a1" into (r, g, b).
    pub fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some((r, g, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_theme() {
        let theme = TerminalTheme::default();
        assert_eq!(theme.name, "Catppuccin Mocha");
        assert_eq!(theme.colors.len(), 16);
    }

    #[test]
    fn test_parse_hex_valid() {
        let (r, g, b) = TerminalTheme::parse_hex("#a6e3a1").unwrap();
        assert_eq!(r, 0xa6);
        assert_eq!(g, 0xe3);
        assert_eq!(b, 0xa1);
    }

    #[test]
    fn test_parse_hex_invalid() {
        assert!(TerminalTheme::parse_hex("not-a-color").is_none());
        assert!(TerminalTheme::parse_hex("#12345").is_none());
    }
}
