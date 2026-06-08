use gpui::prelude::*;
use gpui::*;
use gpui_component::{h_flex, v_flex};
use gpui::ElementId;
use crate::ui::app::{AppState, Nav, TabState};
use crate::ssh::session::SshSession;


pub fn key_to_terminal_bytes(event: &gpui::KeyDownEvent) -> Vec<u8> {
    let key = &event.keystroke.key;
    let modifiers = &event.keystroke.modifiers;

    // Ctrl+letter → control character (0x01–0x1A)
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
        // Printable ASCII — send as-is
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

pub fn render_terminal_area(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    if state.tabs.is_empty() { return div().into_any_element(); }
    let tab = &state.tabs[state.active_tab];
    let terminal = tab.terminal.clone();
    let connected = tab.connected;
    let session = tab.session.clone();
    let focus = state.focus_handle.clone();

    let (lines, _cursor_row, _cursor_col) = {
        let mut term = terminal.lock();
        term.visible_lines()
    };

    v_flex().flex_1().size_full().bg(gpui::rgb(0x0d1117))
        .cursor(gpui::CursorStyle::IBeam)
        .track_focus(&focus)
        .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _window, cx| {
            if let Some(ref sess) = &session {
                let bytes = key_to_terminal_bytes(event);
                if !bytes.is_empty() {
                    let sess = sess.clone();
                    let data = bytes.clone();
                    cx.spawn(async move |_entity: gpui::WeakEntity<AppState>, _cx| {
                        let mut s = sess.lock();
                        let _ = s.send(&data).await;
                    }).detach();
                    if let Some(tab) = this.tabs.get(this.active_tab) {
                        tab.terminal.lock().write(&bytes);
                    }
                }
                cx.notify();
            }
        }))
        .child(
            div().flex_1().p_2().overflow_hidden()
                .font_family("monospace")
                .text_xs()
                .text_color(if connected { gpui::rgb(0xc9d1d9) } else { gpui::rgb(0x6e7681) })
                .children(lines.iter().map(|l| {
                    div().whitespace_nowrap().child(l.clone())
                }))
        ).into_any_element()
}

pub fn render_tab_bar(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    div().id("tab-bar").h_9().bg(rgb(0x141929)).border_b_1().border_color(rgb(0x1e2538)).flex().overflow_x_scroll()
        .children(state.tabs.iter().enumerate().map(|(i, tab)| {
            let act = i == state.active_tab; let ti = i;
            h_flex().id(ElementId::Integer(i as u64)).h_full().px_4().gap_2().border_r_1().border_color(rgb(0x1e2538))
                .when(act, |d| d.bg(rgb(0x0d1117)).border_b_2().border_color(rgb(0x5b7cf6)))
                .cursor_pointer().on_click(cx.listener(move |this, _, window, cx| {
                    this.active_tab = ti;
                    this.nav = Nav::Terminal;
                    this.focus_handle.focus(window, cx);
                    cx.notify();
                }))
                .child(div().size_2().rounded_full().bg(rgb(if tab.connected { 0x22c55e } else { 0xeab308 })))
                .child(div().text_xs().text_color(rgb(if act { 0xdde3f8 } else { 0x7b84a8 })).child(tab.host_label.clone()))
                .child(div().id(ElementId::Name(format!("x-{i}").into())).size_4().flex().items_center().justify_center()
                    .rounded_sm().text_xs().text_color(rgb(0x7b84a8)).hover(|d| d.bg(rgb(0x232942)).text_color(rgb(0xdde3f8)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| { this.close_tab(ti, cx); }))
                    .on_mouse_down(gpui::MouseButton::Middle, cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                        this.close_tab(ti, cx);
                    }))
                    .child("\u{00D7}"))
        }))
}