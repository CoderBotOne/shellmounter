#![allow(dead_code)]
// Terminal emulation bridge: alacritty_terminal ↔ GPUI.
// Wraps alacritty_terminal's Term for rendering SSH output in GPUI.

use alacritty_terminal::{
    event::{Event as TermEvent, EventListener},
    grid::Dimensions,
    index::Direction,
    sync::FairMutex,
    term::{
        search::{RegexIter, RegexSearch},
        Term, TermMode,
    },
    vte::ansi::Processor as VteProcessor,
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

/// A rendered terminal cell with ANSI color attributes.
#[derive(Clone, Debug)]
pub struct TerminalCell {
    pub c: char,
    pub fg: (u8, u8, u8),
    pub bg: (u8, u8, u8),
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Event proxy for alacritty_terminal.
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
    parser: VteProcessor,
    #[allow(dead_code)]
    selection: Option<alacritty_terminal::selection::Selection>,
    scroll_offset: usize,
    size: TerminalSize,
    #[allow(dead_code)]
    search: Option<RegexSearch>,
    fg_color: (u8, u8, u8),
    bg_color: (u8, u8, u8),
    #[allow(dead_code)]
    cursor_color: (u8, u8, u8),
    #[allow(dead_code)]
    selection_color: (u8, u8, u8),
    /// Cache of last visible cells (avoids recomputing every frame).
    cached_cells: Vec<Vec<TerminalCell>>,
    /// Set to true when new data is written.
    dirty: bool,
}

impl TerminalView {
    pub fn new(size: TerminalSize) -> Self {
        let config = alacritty_terminal::term::Config::default();
        let term_size = alacritty_terminal::term::test::TermSize::new(size.cols, size.rows);
        let term = Term::new(config, &term_size, EventProxy);

        let bg = (30, 30, 46);
        let fg = (205, 214, 244);
        let cursor = (245, 224, 220);
        let sel = (88, 91, 112);

        Self {
            term: Arc::new(FairMutex::new(term)),
            parser: VteProcessor::new(),
            selection: None,
            scroll_offset: 0,
            size,
            search: None,
            fg_color: fg,
            bg_color: bg,
            cursor_color: cursor,
            selection_color: sel,
            cached_cells: vec![],
            dirty: true,
        }
    }

    /// Write data to the terminal (from SSH stdout).
    pub fn write(&mut self, data: &[u8]) {
        let mut term = self.term.lock();
        for &byte in data {
            self.parser.advance(&mut *term, byte);
        }
        self.dirty = true; // Invalidate render cache
    }

    /// Resize the terminal grid.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.size = TerminalSize::new(cols, rows);
        let mut term = self.term.lock();
        term.resize(alacritty_terminal::term::test::TermSize::new(cols, rows));
        self.dirty = true;
        self.cached_cells.clear();
    }

    /// Get the currently selected text.
    pub fn get_selection_text(&self) -> Option<String> {
        let term = self.term.lock();
        if self.selection.is_some() {
            term.selection_to_string()
        } else {
            None
        }
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
        let total = term.total_lines();
        let visible = self.size.rows;
        if total > visible {
            let max = total - visible;
            self.scroll_offset =
                (self.scroll_offset as isize + delta).clamp(0, max as isize) as usize;
        }
    }

    /// Scroll to bottom (follow output).
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Get visible cells from the terminal grid.
    /// Returns (cells per row, cursor_row, cursor_col).
    /// Cached: only recomputes when `dirty` is true (new data arrived).
    pub fn visible_cells(&mut self) -> (Vec<Vec<TerminalCell>>, usize, usize) {
        if !self.dirty {
            return (self.cached_cells.clone(), 0, 0);
        }
        let term = self.term.lock();
        let grid = term.grid();
        let total = grid.total_lines();
        let visible = self.size.rows;

        let cursor = grid.cursor.point;
        let cursor_row = cursor.line.0 as usize;
        let cursor_col = cursor.column.0 as usize;

        let start = if total > visible {
            total - visible - self.scroll_offset
        } else {
            0
        };

        // Build rows with default theme colors
        let default_cell = TerminalCell {
            c: ' ',
            fg: self.fg_color,
            bg: self.bg_color,
            bold: false,
            italic: false,
            underline: false,
        };

        let mut rows: Vec<Vec<TerminalCell>> = (0..visible)
            .map(|_| vec![default_cell.clone(); self.size.cols])
            .collect();

        for cell in grid.display_iter() {
            if cell.point.line.0 < 0 {
                continue;
            }
            let row = cell.point.line.0 as usize;
            if row < start {
                continue;
            }
            let rel_row = row - start;
            if rel_row >= visible {
                break;
            }
            let col = cell.point.column.0 as usize;
            if col < self.size.cols {
                rows[rel_row][col] = TerminalCell {
                    c: cell.c,
                    fg: self.fg_color,
                    bg: self.bg_color,
                    bold: cell.flags.contains(alacritty_terminal::term::cell::Flags::BOLD),
                    italic: cell.flags.contains(alacritty_terminal::term::cell::Flags::ITALIC),
                    underline: cell.flags.contains(alacritty_terminal::term::cell::Flags::UNDERLINE),
                };
            }
        }
        drop(term);

        self.cached_cells = rows.clone();
        self.dirty = false;
        (rows, cursor_row, cursor_col)
    }

    /// Backward-compatible: returns plain-text lines string.
    pub fn visible_lines(&mut self) -> (Vec<String>, usize, usize) {
        let (cells, cr, cc) = self.visible_cells();
        let lines: Vec<String> = cells
            .iter()
            .map(|row| {
                let s: String = row.iter().map(|c| c.c).collect();
                s.trim_end().to_string()
            })
            .collect();
        (lines, cr, cc)
    }

    /// Get terminal dimensions.
    pub fn dimensions(&self) -> TerminalSize {
        self.size
    }

    /// Apply a color theme (RGB tuples).
    pub fn set_theme(
        &mut self,
        fg: (u8, u8, u8),
        bg: (u8, u8, u8),
        cursor: (u8, u8, u8),
        sel: (u8, u8, u8),
    ) {
        self.fg_color = fg;
        self.bg_color = bg;
        self.cursor_color = cursor;
        self.selection_color = sel;
        // Resize to trigger redraw with new bg color
        let cols = self.size.cols;
        let rows = self.size.rows;
        let mut term = self.term.lock();
        term.resize(alacritty_terminal::term::test::TermSize::new(cols, rows));
        self.dirty = true;
    }

    /// Find text in terminal.
    pub fn search_next(&mut self, pattern: &str) -> Option<alacritty_terminal::index::Point> {
        let term = self.term.lock();
        let search = self
            .search
            .get_or_insert_with(|| RegexSearch::new(pattern).expect("valid regex"));
        let start = term.grid().cursor.point;
        let end = alacritty_terminal::index::Point::new(
            alacritty_terminal::index::Line(0),
            alacritty_terminal::index::Column(0),
        );
        let mut iter = RegexIter::new(start, end, Direction::Right, &term, search);
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
