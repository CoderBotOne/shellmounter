#![allow(unused)]
use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    tab::{TabBar, Tab, TabVariant},
    h_flex, v_flex, ActiveTheme,
};

use crate::ui::app::{AppState, Nav};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputMode { Shell, Ai }

/// Render the AI input bar with real text input and Send button.
pub fn render_input_bar(
    mode: InputMode,
    has_api_key: bool,
    input_text: SharedString,
    cx: &mut Context<AppState>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let display: SharedString = if input_text.is_empty() { "Type a message...".into() } else { input_text.clone() };

    v_flex()
        .border_t_1().border_color(theme.border).bg(theme.background).px_3().py_2()
        .child(
            h_flex().gap_1().px_1()
                .child(div().text_xs().text_color(theme.muted_foreground)
                    .child(if mode == InputMode::Ai { "AI Chat" } else { "Shell" }))
                .child(
                    div().text_xs().text_color(if has_api_key { rgb(0x22c55e) } else { rgb(0xef4444) })
                        .child(if has_api_key { "ready" } else { "no API key" })))
        .child(
            h_flex().gap_2().items_center().mt_1()
                .child(
                    // Text display area (also captures keyboard)
                    div().flex_1().px_3().py_1().rounded_lg().bg(hsla(0.0, 0.0, 0.0, 1.0))
                        .id("ai-text-display")
                        .child(
                            div().text_sm()
                                .text_color(if input_text.is_empty() {
                                    hsla(0.0, 0.0, 0.42, 1.0)
                                } else {
                                    theme.foreground
                                })
                                .child(display)))
                .child(
                    Button::new("send-ai").primary().child("Send")
                        .on_click(cx.listener(|this: &mut AppState, _, _w: &mut Window, cx| {
                            let text = std::mem::take(&mut this.ai_text);
                            if !text.trim().is_empty() {
                                this.send_ai_message(text, cx);
                            }
                            cx.notify();
                        })))
                .child(mode_tabs(mode, cx)))
}

fn mode_tabs(mode: InputMode, cx: &mut Context<AppState>) -> impl IntoElement {
    TabBar::new("mode-tabs")
        .with_variant(TabVariant::Underline)
        .selected_index(if mode == InputMode::Shell { 0 } else { 1 })
        .child(
            Tab::new().child("Shell")
                .on_click(cx.listener(|this: &mut AppState, _, _, cx| {
                    this.input_mode = InputMode::Shell;
                    this.nav = Nav::Terminal;
                    cx.notify();
                })))
        .child(
            Tab::new().child("AI")
                .on_click(cx.listener(|this: &mut AppState, _, _, cx| {
                    this.input_mode = InputMode::Ai;
                    this.nav = Nav::Termia;
                    cx.notify();
                })))
}
