use gpui::prelude::*;
use gpui::*;
use gpui_component::{h_flex, v_flex, scroll::ScrollableElement as _};
use crate::ui::app::AppState;


pub fn render_known_hosts_view(state: &AppState) -> impl IntoElement {
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(gpui::rgb(0x2a2f45))
            .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("Known Hosts"))
            .child(div().flex_1())
            .child(div().text_xs().text_color(gpui::rgb(0x7b84a8)).child(format!("{} entries", state.known_host_entries.len()))))
        .child(div().flex_1().overflow_y_scrollbar().p_4().font_family("monospace").text_xs().text_color(gpui::rgb(0x9aa3bf))
            .children(state.known_host_entries.iter().map(|e| div().py_1().child(e.clone())).collect::<Vec<_>>()))
}