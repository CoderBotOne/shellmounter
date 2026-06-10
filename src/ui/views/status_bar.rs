#![allow(unused)]
use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, ActiveTheme,
};
use crate::ui::app::{AppState, Nav};

pub fn render_status_bar(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let theme = cx.theme().clone();
    h_flex().h_6().px_4().gap_2().border_t_1().border_color(theme.border).bg(theme.title_bar)
        .child(div().text_xs().text_color(theme.muted_foreground).child(state.status_message.clone()))
        .child(div().flex_1())
        .child(
            Button::new("explain-terminal").ghost().child("AI Explain")
                .on_click(cx.listener(|this: &mut AppState, _, _, cx| {
                    this.send_ai_message("Explain the terminal output and what's happening".to_string(), cx);
                    this.nav = Nav::Termia;
                    cx.notify();
                })))
        .child(
            Button::new("toggle-ai-mini").ghost()
                .child(if state.ai_mini_visible { "AI on" } else { "AI" })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.ai_mini_visible = !this.ai_mini_visible;
                    cx.notify();
                })))
        .child(div().text_xs().text_color(theme.muted_foreground)
            .child(format!("v{} | Ctrl+K", env!("CARGO_PKG_VERSION"))))
}
