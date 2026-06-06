//! Terminal emulation bridge: alacritty_terminal ↔ GPUI.
//!
//! Wraps alacritty_terminal's Term for rendering SSH output in GPUI.
//! Handles ANSI/VT input parsing, grid state, scrollback, and selection.
//!
//! Compatible with alacritty_terminal 0.26+ API.

use alacritty_terminal::{
    event::{Event as TermEvent, EventListener},
    grid::Scroll,
    selection::Selection,
    sync::FairMutex,
    term::{
        cell::Flags,
        search::{RegexIter, RegexSearch},
        Term, TermMode,
    },
};
use std::sync::Arc;

/// Terminal dimensions in cells.
#[derive(Clone, Copy, Debug)]
pub struct TerminalSize {
    pub cols: usize,
    pub rows: usize,
}

impl TerminalSize {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self { cols, rows }
    }
}

/// Event proxy for alacritty_terminal (notifies on bell, title change, etc.)
struct EventProxy;

impl EventListener for EventProxy {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::Title(title) => log::debug!("Terminal title: {}", title),
            TermEvent::Bell => log::debug!("Terminal bell"),
            _ => {}
        }
    }
}

/// The terminal view — wraps alacritty_terminal for GPUI rendering.
pub struct TerminalView {
    term: Arc<FairMutex<Term<EventProxy>>>,
    selection: Option<Selection>,
    scroll_offset: usize,
    size: TerminalSize,
    search: Option<RegexSearch>,
    fg_color: (u8, u8, u8),
    bg_color: (u8, u8, u8),
    cursor_color: (u8, u8, u8),
    selection_color: (u8, u8, u8),
}

impl TerminalView {
    /// Create a new terminal view with default config.
    pub fn new(size: TerminalSize) -> Self {
        let config = alacritty_terminal::term::Config::default();
        let term_size = alacritty_terminal::term::test::TermSize::new(size.cols, size.rows);
        let term = Term::new(config, &term_size, EventProxy);

        // Default colors (Catppuccin Mocha)
        let bg = (30, 30, 46);   // #1e1e2e
        let fg = (205, 214, 244); // #cdd6f4
        let cursor = (245, 224, 220); // #f5e0dc
        let sel = (88, 91, 112);  // #585b70

        Self {
            term: Arc::new(FairMutex::new(term)),
            selection: None,
            scroll_offset: 0,
            size,
            search: None,
            fg_color: fg,
            bg_color: bg,
            cursor_color: cursor,
            selection_color: sel,
        }
    }

    /// Write data to the terminal (from SSH stdout).
    pub fn write(&mut self, data: &[u8]) {
        let mut term = self.term.lock();
        // In 0.26, write directly to the term — VTE processing is built-in
        term.write(data);
    }

    /// Resize the terminal grid.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.size = TerminalSize::new(cols, rows);
        let mut term = self.term.lock();
        term.resize(alacritty_terminal::term::test::TermSize::new(cols, rows));
    }

    /// Get the current terminal contents as text.
    pub fn get_selection_text(&self) -> Option<String> {
        let term = self.term.lock();
        self.selection
            .as_ref()
            .map(|sel| term.selection_to_string(sel))
            .filter(|s| !s.is_empty())
    }

    /// Handle keyboard input.
    pub fn handle_input(&mut self, text: &str) -> Vec<u8> {
        text.as_bytes().to_vec()
    }

    /// Check if terminal is in a specific mode.
    pub fn in_mode(&self, mode: TermMode) -> bool {
        self.term.lock().mode().contains(mode)
    }

    /// Scroll in scrollback.
    pub fn scroll(&mut self, delta: isize) {
        let term = self.term.lock();
        let total = term.grid().total_lines();
        let visible = self.size.rows;
        if total > visible {
            let max = total - visible;
            self.scroll_offset = (self.scroll_offset as isize + delta).clamp(0, max as isize) as usize;
        }
    }

    /// Reset scroll to follow output.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Find text in terminal.
    pub fn search_next(&mut self, pattern: &str) -> Option<alacritty_terminal::index::Point> {
        let mut term = self.term.lock();
        let search = self
            .search
            .get_or_insert_with(|| RegexSearch::new(pattern).expect("valid regex"));
        let start = term.grid().cursor.point;
        let end = alacritty_terminal::index::Point::new(
            alacritty_terminal::index::Line(0),
            alacritty_terminal::index::Column(0),
        );
        let mut iter = RegexIter::new(start, end, &term, search);
        iter.next().map(|range| *range.start())
    }

    pub fn clear_search(&mut self) {
        self.search = None;
        self.selection = None;
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_create() {
        let term = TerminalView::new(TerminalSize::new(80, 24));
        assert_eq!(term.size.cols, 80);
        assert_eq!(term.size.rows, 24);
    }

    #[test]
    fn test_terminal_write() {
        let mut term = TerminalView::new(TerminalSize::new(80, 24));
        term.write(b"Hello World\r\n");
        // Should not crash
    }

    #[test]
    fn test_terminal_resize() {
        let mut term = TerminalView::new(TerminalSize::new(80, 24));
        term.resize(120, 40);
        assert_eq!(term.size.cols, 120);
        assert_eq!(term.size.rows, 40);
    }

    #[test]
    fn test_scroll_bounds() {
        let mut term = TerminalView::new(TerminalSize::new(80, 24));
        term.scroll(10);
        assert_eq!(term.scroll_offset, 0);
        term.scroll_to_bottom();
        assert_eq!(term.scroll_offset, 0);
    }
}
