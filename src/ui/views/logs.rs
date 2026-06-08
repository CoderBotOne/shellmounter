use gpui::prelude::*;
use gpui::*;
use gpui_component::{h_flex, v_flex, scroll::ScrollableElement as _, ActiveTheme};
use gpui::FontWeight;
use crate::ui::app::AppState;


pub fn render_logs_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("Logs"))
            .child(div().flex_1())
            .child(div().id("refresh-logs").text_xs().text_color(cx.theme().primary).cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.log_lines = AppState::load_logs(&this.data_dir); cx.notify();
                })).child("Refresh")))
        .child(div().flex_1().overflow_y_scrollbar().p_4().font_family("monospace").text_xs().text_color(cx.theme().muted_foreground)
            .children(state.log_lines.iter().map(|l| div().py_0p5().child(l.clone())).collect::<Vec<_>>()))
}