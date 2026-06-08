use gpui::prelude::*;
use gpui::*;
use gpui_component::{h_flex, ActiveTheme};
use crate::ui::app::AppState;


pub fn render_status_bar(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    h_flex().h_6().px_4().gap_2().border_t_1().border_color(cx.theme().border).bg(cx.theme().title_bar)
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(state.status_message.clone()))
        .child(div().flex_1())
        .child(div().text_xs().text_color(cx.theme().muted_foreground)
            .child(format!("{} hosts · v{}", state.hosts.len(), env!("CARGO_PKG_VERSION"))))
}