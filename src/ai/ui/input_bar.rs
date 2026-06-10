use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    badge::Badge,
    button::{Button, ButtonVariants as _},
    tab::{TabBar, Tab},
    h_flex, v_flex, ActiveTheme,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputMode { Shell, Ai }

/// Render the Shell/AI input bar.
pub fn render_input_bar(
    mode: InputMode,
    input_text: String,
    on_send: impl Fn(String, &mut Window, &mut App) + 'static,
    cx: &mut Context<crate::ui::app::AppState>,
) -> impl IntoElement {
    let theme = cx.theme().clone();
    let text_for_send = input_text.clone();
    let display = if input_text.is_empty() { "Ask Termia...".to_string() } else { input_text };

    if mode == InputMode::Shell {
        let green = hsla(142.0, 0.71, 0.45, 1.0);
        v_flex().border_t_1().border_color(theme.border).bg(theme.background).px_3().py_2()
            .child(h_flex().gap_1().px_1()
                .child(Badge::new().child("~/proyectos/termia"))
                .child(Badge::new().child("main")))
            .child(h_flex().gap_2().items_end().mt_1()
                .child(div().flex_1().px_3().py_1().rounded_lg().bg(hsla(0.0,0.0,0.0,1.0))
                    .child(div().text_sm().text_color(green).child("$ ")))
                .child(mode_tabs(mode)))
            .into_any_element()
    } else {
        v_flex().border_t_1().border_color(theme.border).bg(theme.background).px_3().py_2()
            .child(h_flex().gap_1().px_1()
                .child(Badge::new().child("~/proyectos/termia"))
                .child(Badge::new().child("main")))
            .child(h_flex().gap_2().items_center().mt_1()
                .child(div().flex_1().px_3().py_1().rounded_lg().bg(hsla(0.0,0.0,0.0,1.0))
                    .child(div().text_sm().text_color(hsla(0.0,0.0,0.42,1.0)).child(display)))
                .child(Button::new("send-ai").primary().child("Send")
                    .on_click(move |_, w, cx| { on_send(text_for_send.clone(), w, cx); }))
                .child(mode_tabs(mode)))
            .into_any_element()
    }
}

fn mode_tabs(_mode: InputMode) -> impl IntoElement {
    TabBar::new("mode-tabs")
        .child(Tab::new().child("Shell"))
        .child(Tab::new().child("AI"))
}
