use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    badge::Badge,
    h_flex, ActiveTheme,
};
use crate::ui::app::AppState;


pub fn render_status_bar(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let theme = cx.theme().clone();
    h_flex().h_6().px_4().gap_2().border_t_1().border_color(theme.border).bg(theme.title_bar)
        .child(div().text_xs().text_color(theme.muted_foreground).child(state.status_message.clone()))
        .child(div().flex_1())
        .child(
            Button::new("toggle-ai-mini")
                .ghost()
                .child(if state.ai_mini_visible { "AI on" } else { "AI" })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.ai_mini_visible = !this.ai_mini_visible;
                    cx.notify();
                }))
        )
        .child(div().text_xs().text_color(theme.muted_foreground)
            .child(format!("{} hosts · v{}", state.hosts.len(), env!("CARGO_PKG_VERSION"))))
}
