use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme,
};
use gpui_component::scroll::ScrollableElement as _;

use crate::ai::chat::ChatState;

/// Render the floating AI mini window overlay.
pub fn render_mini_window(
    state: &ChatState,
    visible: bool,
    cx: &mut Context<crate::ui::app::AppState>,
) -> impl IntoElement {
    let theme = cx.theme().clone();

    if !visible {
        return div().into_any_element();
    }

    div()
        .absolute().right_0().bottom_12().w(px(420.0)).h(px(520.0))
        .rounded_2xl().border_1().border_color(theme.border).bg(theme.background).shadow_md()
        .child(
            v_flex().h_full()
                .child(mini_header(&theme))
                .child(plan_strip(&theme))
                .child(
                    div().flex_1().min_h_0().overflow_y_scrollbar().p_2().child(
                        if state.messages.is_empty() {
                            empty_suggestions(&theme)
                        } else {
                            div().text_sm().text_color(theme.foreground).child("Messages...").into_any_element()
                        }
                    )
                )
                .child(
                    div().border_t_1().border_color(theme.border).px_2().py_1()
                        .child(div().text_xs().text_color(theme.muted_foreground).child("Todo: no tasks"))
                ),
        )
        .into_any_element()
}

fn mini_header(theme: &gpui_component::Theme) -> impl IntoElement {
    h_flex().h_11().items_center().justify_between().gap_2().border_b_1().border_color(theme.border).px_3()
        .child(h_flex().gap_1().items_center()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child("Termia AI"))
        )
        .child(
            Button::new("mini-close").ghost().child("X")
        )
}

fn plan_strip(theme: &gpui_component::Theme) -> impl IntoElement {
    let amber = hsla(45.0, 0.93, 0.47, 1.0);
    div().border_b_1().border_color(theme.border).bg(theme.muted_foreground.blend(theme.background)).px_3().py_1()
        .child(h_flex().gap_2().items_center()
            .child(div().size_1p5().rounded_full().bg(amber))
            .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child("Plan mode"))
            .child(div().text_xs().text_color(theme.muted_foreground).child("- no edits queued"))
        )
}

fn empty_suggestions(_theme: &gpui_component::Theme) -> AnyElement {
    let muted = hsla(0.0, 0.0, 0.62, 1.0);
    v_flex().flex_1().items_center().justify_center().gap_3()
        .child(div().text_sm().text_color(muted).child("Ask Termia anything"))
        .child(Button::new("sug-1").ghost().child("Explain the last error"))
        .child(Button::new("sug-2").ghost().child("Generate a command"))
        .child(Button::new("sug-3").ghost().child("Summarize buffer"))
        .into_any_element()
}
