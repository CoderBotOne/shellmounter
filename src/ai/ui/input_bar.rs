#![allow(unused)]
use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    tab::{TabBar, Tab},
    h_flex, v_flex, ActiveTheme,
};

use crate::ui::app::AppState;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputMode { Shell, Ai }

/// Render the AI input bar with Send button.
pub fn render_input_bar(
    _mode: InputMode,
    has_api_key: bool,
    cx: &mut Context<AppState>,
) -> impl IntoElement {
    let theme = cx.theme().clone();

    v_flex()
        .border_t_1().border_color(theme.border).bg(theme.background).px_3().py_2()
        .child(
            h_flex().gap_1().px_1()
                .child(div().text_xs().text_color(theme.muted_foreground).child("AI Chat"))
                .child(
                    div().text_xs().text_color(if has_api_key { rgb(0x22c55e) } else { rgb(0xef4444) })
                        .child(if has_api_key { "ready" } else { "no API key" })))
        .child(
            h_flex().gap_2().items_center().mt_1()
                .child(div().flex_1())
                .child(
                    Button::new("send-ai").primary().child("Send")
                        .on_click(cx.listener(|this: &mut AppState, _, _w: &mut Window, cx| {
                            let text = "Explain this project".to_string();
                            this.send_ai_message(text, cx);
                            cx.notify();
                        })))
                .child(mode_tabs(_mode)))
}

fn mode_tabs(_mode: InputMode) -> impl IntoElement {
    TabBar::new("mode-tabs")
        .child(Tab::new().child("Shell"))
        .child(Tab::new().child("AI"))
}
