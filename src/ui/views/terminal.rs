#![allow(unused)]
use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    h_flex, v_flex,
    tab::{Tab, TabBar, TabVariant},
    ActiveTheme, Icon, IconName, Sizable,
};
use crate::ui::app::{AppState, Nav, TabState};
use crate::terminal::view::TerminalCell;

pub fn key_to_terminal_bytes(event: &gpui::KeyDownEvent) -> Vec<u8> {
    let key = &event.keystroke.key;
    let modifiers = &event.keystroke.modifiers;

    if modifiers.control && key.len() == 1 {
        let c = key.chars().next().unwrap();
        if c.is_ascii_uppercase() || c.is_ascii_lowercase() {
            return vec![c.to_ascii_lowercase() as u8 & 0x1f];
        }
    }

    match key.as_str() {
        "enter" | "return" => vec![b'\r'],
        "backspace" => vec![0x7f],
        "tab" => vec![b'\t'],
        "escape" => vec![0x1b],
        "space" => vec![b' '],
        "up" => vec![0x1b, b'[', b'A'],
        "down" => vec![0x1b, b'[', b'B'],
        "right" => vec![0x1b, b'[', b'C'],
        "left" => vec![0x1b, b'[', b'D'],
        "home" => vec![0x1b, b'[', b'H'],
        "end" => vec![0x1b, b'[', b'F'],
        "delete" => vec![0x1b, b'[', b'3', b'~'],
        "pageup" => vec![0x1b, b'[', b'5', b'~'],
        "pagedown" => vec![0x1b, b'[', b'6', b'~'],
        "f1" => vec![0x1b, b'O', b'P'],
        "f2" => vec![0x1b, b'O', b'Q'],
        "f3" => vec![0x1b, b'O', b'R'],
        "f4" => vec![0x1b, b'O', b'S'],
        "f5" => vec![0x1b, b'[', b'1', b'5', b'~'],
        "f6" => vec![0x1b, b'[', b'1', b'7', b'~'],
        "f7" => vec![0x1b, b'[', b'1', b'8', b'~'],
        "f8" => vec![0x1b, b'[', b'1', b'9', b'~'],
        "f9" => vec![0x1b, b'[', b'2', b'0', b'~'],
        "f10" => vec![0x1b, b'[', b'2', b'1', b'~'],
        "f11" => vec![0x1b, b'[', b'2', b'3', b'~'],
        "f12" => vec![0x1b, b'[', b'2', b'4', b'~'],
        other if other.len() == 1 => {
            let c = other.chars().next().unwrap();
            if c.is_ascii() && !c.is_ascii_control() {
                vec![c as u8]
            } else {
                vec![]
            }
        }
        _ => vec![],
    }
}

fn render_cell_row(row: &[TerminalCell]) -> Vec<AnyElement> {
    if row.is_empty() {
        return vec![div().child(" ").into_any_element()];
    }

    let mut spans: Vec<AnyElement> = Vec::new();
    let mut run_start = 0;
    let mut last_bg = row[0].bg;
    let mut last_fg = row[0].fg;
    let mut last_bold = row[0].bold;

    for (i, cell) in row.iter().enumerate().skip(1) {
        if cell.bg != last_bg || cell.fg != last_fg || cell.bold != last_bold {
            let run_text: String = row[run_start..i].iter().map(|c| c.c).collect();
            let bg_color = gpui::rgb((last_bg.0 as u32) << 16 | (last_bg.1 as u32) << 8 | last_bg.2 as u32);
            let fg_color = gpui::rgb((last_fg.0 as u32) << 16 | (last_fg.1 as u32) << 8 | last_fg.2 as u32);
            let mut span = div()
                .bg(bg_color)
                .text_color(fg_color)
                .whitespace_nowrap();
            if last_bold {
                span = span.font_weight(gpui::FontWeight::BOLD);
            }
            spans.push(span.child(run_text).into_any_element());
            run_start = i;
            last_bg = cell.bg;
            last_fg = cell.fg;
            last_bold = cell.bold;
        }
    }

    let run_text: String = row[run_start..].iter().map(|c| c.c).collect();
    let bg_color = gpui::rgb((last_bg.0 as u32) << 16 | (last_bg.1 as u32) << 8 | last_bg.2 as u32);
    let fg_color = gpui::rgb((last_fg.0 as u32) << 16 | (last_fg.1 as u32) << 8 | last_fg.2 as u32);
    let mut span = div()
        .bg(bg_color)
        .text_color(fg_color)
        .whitespace_nowrap();
    if last_bold {
        span = span.font_weight(gpui::FontWeight::BOLD);
    }
    spans.push(span.child(run_text).into_any_element());

    spans
}

pub fn render_terminal_area(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    if state.tabs.is_empty() { return div().into_any_element(); }
    let tab = &state.tabs[state.active_tab];
    let terminal = tab.terminal.clone();
    let pty = tab.pty.clone();
    let focus = state.focus_handle.clone();
    let font_size = state.terminal_font_size;

    let cell_rows: Vec<Vec<TerminalCell>> = {
        let mut term = terminal.lock();
        let (cells, _, _) = term.visible_cells();
        cells
    };

    let term_bg = gpui::rgb(0x0d1117_u32);

    v_flex().flex_1().size_full().bg(term_bg)
        .cursor(gpui::CursorStyle::IBeam)
        .track_focus(&focus)
        .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _window, cx| {
            let bytes = key_to_terminal_bytes(event);
            if !bytes.is_empty() {
                // Send to local PTY
                if let Some(ref pty) = pty {
                    let mut p = pty.lock();
                    let _ = p.write_all(&bytes);
                }
                // Also echo locally in terminal emulator
                if let Some(tab) = this.tabs.get(this.active_tab) {
                    tab.terminal.lock().write(&bytes);
                }
                cx.notify();
            }
        }))
        .child(
            div().flex_1().p_2().overflow_hidden()
                .font_family("monospace")
                .text_size(rems((font_size as f32) / 16.0))
                .text_color(gpui::rgb(0xc9d1d9))
                .children(cell_rows.iter().map(|row| {
                    h_flex().children(render_cell_row(row))
                }))
        ).into_any_element()
}

pub fn render_tab_bar(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    TabBar::new("sessions")
        .with_variant(TabVariant::Underline)
        .selected_index(state.active_tab)
        .children(state.tabs.iter().enumerate().map(|(i, tab)| {
            let ti = i;
            let label = tab.label.clone();
            let has_pty = tab.pty.is_some();
            Tab::new()
                .label(label.clone())
                .prefix(
                    div().size_2().rounded_full().flex_shrink_0()
                        .bg(gpui::rgb(if has_pty { 0x22c55e } else { 0xeab308 }))
                )
                .suffix(
                    div().id(ElementId::Name(format!("close-tab-{i}").into()))
                        .cursor_pointer()
                        .child(Icon::new(IconName::Close).size_3()
                            .text_color(gpui::rgb(0x7b84a8)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_tab(ti, cx);
                        }))
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.active_tab = ti;
                    this.nav = Nav::Terminal;
                    this.focus_handle.focus(window, cx);
                    cx.notify();
                }))
        }))
}
