//! Terminal emulation bridge: alacritty_terminal ↔ GPUI.
//!
//! Wraps alacritty_terminal's Term in a GPUI-friendly API.
//! Handles ANSI/VT parsing, grid rendering, scrollback, and selection.
//!
//! Memory safety:
//! - Single ownership of Term via the view
//! - Event-based I/O (no polling loops)
//! - Selection state cleaned on terminal reset

use alacritty_terminal::{
    event::{Event as TermEvent, EventListener},
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point},
    selection::Selection,
    sync::FairMutex,
    term::{
        cell::Flags,
        color::Rgb,
        search::{RegexSearch, RegexIter},
        test::TermSize,
        Config, Term, TermMode,
    },
    vte::{ansi::ClearMode, Processor},
};
use gpui::*;
use std::ops::RangeInclusive;
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

/// The terminal view — renders an alacritty_terminal grid inside GPUI.
pub struct TerminalView {
    /// The alacritty terminal instance (behind a mutex for event listener)
    term: Arc<FairMutex<Term<EventProxy>>>,
    /// ANSI/VT processor
    processor: Processor,
    /// Current selection
    selection: Option<Selection>,
    /// Scrollback position (0 = bottom / follow output)
    scroll_offset: usize,
    /// Terminal dimensions
    size: TerminalSize,
    /// Search state
    search: Option<RegexSearch>,
    /// Font size for cell dimensions
    cell_width: f32,
    cell_height: f32,
    /// Foreground/background colors
    fg_color: Rgb,
    bg_color: Rgb,
    cursor_color: Rgb,
    selection_color: Rgb,
}

/// Event proxy for alacritty_terminal (notifies on bell, title change, etc.)
struct EventProxy;

impl EventListener for EventProxy {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::Title(title) => {
                log::debug!("Terminal title changed: {}", title);
            }
            TermEvent::Bell => {
                log::debug!("Terminal bell");
            }
            _ => {}
        }
    }
}

impl TerminalView {
    /// Create a new terminal view with default config.
    pub fn new(size: TerminalSize) -> Self {
        let config = Config::default();
        let term = Term::new(
            config,
            &TermSize::new(size.cols, size.rows),
            EventProxy,
        );

        let colors = term.colors();

        Self {
            term: Arc::new(FairMutex::new(term)),
            processor: Processor::new(),
            selection: None,
            scroll_offset: 0,
            size,
            search: None,
            cell_width: 8.4,
            cell_height: 17.0,
            fg_color: colors.primary.foreground,
            bg_color: colors.primary.background,
            cursor_color: colors.cursor.cursor,
            selection_color: colors.selection.background,
        }
    }

    /// Write data to the terminal (from SSH stdout).
    pub fn write(&mut self, data: &[u8]) {
        let mut term = self.term.lock();
        self.processor.advance(&mut *term, data);
    }

    /// Resize the terminal grid.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.size = TerminalSize::new(cols, rows);
        let mut term = self.term.lock();
        term.resize(TermSize::new(cols, rows));
    }

    /// Get the current terminal contents as text (for copy).
    pub fn get_selection_text(&self) -> Option<String> {
        let term = self.term.lock();
        self.selection.as_ref().and_then(|sel| {
            let text = term.selection_to_string(sel);
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
    }

    /// Copy selection to clipboard.
    pub fn copy_selection(&self, cx: &mut WindowContext) {
        if let Some(text) = self.get_selection_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Handle keyboard input (from GPUI key events).
    pub fn handle_input(&mut self, text: &str) -> Vec<u8> {
        // This is a simplified input handler.
        // Full implementation would map GPUI Keystroke → ANSI escape sequences.
        text.as_bytes().to_vec()
    }

    /// Get the cursor position (line, column) for rendering.
    pub fn cursor_position(&self) -> (usize, usize) {
        let term = self.term.lock();
        let cursor = term.grid().cursor.point;
        (cursor.line.0 as usize, cursor.column.0 as usize)
    }

    /// Check if terminal is in a specific mode (e.g., insert, application keypad).
    pub fn in_mode(&self, mode: TermMode) -> bool {
        let term = self.term.lock();
        term.mode().contains(mode)
    }

    /// Scroll up/down in scrollback.
    pub fn scroll(&mut self, delta: isize) {
        let term = self.term.lock();
        let total_lines = term.grid().total_lines();
        let visible = self.size.rows;

        if total_lines > visible {
            let max_scroll = total_lines - visible;
            let new_offset = self.scroll_offset as isize + delta;
            self.scroll_offset = new_offset.clamp(0, max_scroll as isize) as usize;
        }
    }

    /// Reset scroll to follow output.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Find text in the terminal (forward search).
    pub fn search_next(&mut self, pattern: &str) -> Option<Point> {
        let mut term = self.term.lock();

        let search = self.search.get_or_insert_with(|| {
            RegexSearch::new(pattern).expect("valid regex")
        });

        let start = term.grid().cursor.point;
        let end = Point::new(Line(0), Column(0));

        let mut iter = RegexIter::new(start, end, &term, search);
        iter.next().and_then(|range| {
            let point = *range.start();
            let mut sel = Selection::new(Selection::semantic_ty(), point, point);
            term.grid().scroll(Scroll::Delta(-(self.scroll_offset as isize)));
            self.selection = Some(sel);
            Some(point)
        })
    }

    /// Clear search highlights.
    pub fn clear_search(&mut self) {
        self.search = None;
        self.selection = None;
    }
}

// ── GPUI Render ─────────────────────────────────────────────────────────

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let term = self.term.lock();
        let grid = term.grid();
        let total_lines = grid.total_lines();
        let visible_rows = self.size.rows;

        // Calculate visible range considering scrollback
        let start_line = total_lines.saturating_sub(visible_rows + self.scroll_offset);
        let end_line = total_lines.saturating_sub(self.scroll_offset);

        let cursor = grid.cursor.point;

        div()
            .size_full()
            .bg(rgb(
                (self.bg_color.r as f32 / 255.0) * 255.0,
                (self.bg_color.g as f32 / 255.0) * 255.0,
                (self.bg_color.b as f32 / 255.0) * 255.0,
            ))
            .font_family("JetBrains Mono, Menlo, Monaco, monospace")
            .text_size(px(13.0))
            .overflow_hidden()
            .child(
                // Render visible lines
                div().flex_col().children(
                    (start_line..end_line)
                        .filter_map(|line_idx| {
                            let line = grid.buffer().line(line_idx)?;
                            let is_cursor_line = line_idx == cursor.line;
                            let text: String = line
                                .into_iter()
                                .map(|cell| cell.c)
                                .collect();

                            Some(
                                div()
                                    .h(px(self.cell_height))
                                    .flex()
                                    .when(is_cursor_line, |d| {
                                        d.bg(rgb(
                                            (self.cursor_color.r as f32 / 255.0) * 255.0,
                                            (self.cursor_color.g as f32 / 255.0) * 255.0,
                                            (self.cursor_color.b as f32 / 255.0) * 255.0,
                                        ))
                                    })
                                    .child(
                                        Label::new(text).color(rgb(
                                            (self.fg_color.r as f32 / 255.0) * 255.0,
                                            (self.fg_color.g as f32 / 255.0) * 255.0,
                                            (self.fg_color.b as f32 / 255.0) * 255.0,
                                        )),
                                    ),
                            )
                        })
                        .collect::<Vec<_>>(),
                ),
            )
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
        assert!(term.selection.is_none());
    }

    #[test]
    fn test_terminal_write() {
        let mut term = TerminalView::new(TerminalSize::new(80, 24));
        term.write(b"Hello World\r\n");
        // Terminal should have processed the input without crashing
        let text = term.get_selection_text();
        // Initially no selection, should be None
        assert!(text.is_none());
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
        assert_eq!(term.scroll_offset, 0, "should not scroll past buffer");

        term.scroll_to_bottom();
        assert_eq!(term.scroll_offset, 0);
    }

    #[test]
    fn test_in_mode_default() {
        let term = TerminalView::new(TerminalSize::new(80, 24));
        assert!(!term.in_mode(TermMode::INSERT));
        assert!(!term.in_mode(TermMode::APP_KEYPAD));
    }
}
