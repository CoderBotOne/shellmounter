use gpui::prelude::*;
use gpui::*;
use gpui_component::{h_flex, v_flex, scroll::ScrollableElement as _, ActiveTheme, Icon, Sizable, IconName};
use uuid::Uuid;
use crate::ui::app::AppState;
use crate::ssh::port_forward::{ForwardKind, PortForwardRule};
use super::widgets::{btn, empty};


pub fn render_port_forward_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let rules = state.port_forward.list().to_vec();
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(btn("+ Nueva regla", true, cx, |s, cx| {
                let rule = PortForwardRule {
                    id: Uuid::new_v4().to_string(),
                    label: "nueva".into(),
                    kind: ForwardKind::Local,
                    local_port: 8080,
                    remote_host: "localhost".into(),
                    remote_port: 80,
                    enabled: false,
                };
                s.port_forward.add(rule);
                s.status_message = "Regla agregada".into();
                cx.notify();
            }))
            .child(div().flex_1())
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{} rules", rules.len()))))
        .child(div().flex_1().overflow_y_scrollbar().p_4()
            .children(if rules.is_empty() {
                vec![empty("Sin reglas", "Agrega reglas de port forwarding.", IconName::Network, cx).into_any_element()]
            } else {
                rules.iter().map(|r| {
                    let rid = r.id.clone();
                    h_flex().w_full().px_3().py_2().rounded(cx.theme().radius).gap_3().bg(cx.theme().background)
                        .border_1().border_color(cx.theme().border).mb_1()
                        .child(Icon::new(IconName::Network).small())
                        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(r.label.clone()))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(r.describe())))
                        .child(div().id(format!("del-{rid}")).size_6().rounded(cx.theme().radius).flex().items_center()
                            .justify_center().cursor_pointer().hover(|d| d.bg(rgb(0xef4444)).text_color(rgb(0xffffff)))
                            .text_xs().child("x").on_click(cx.listener(move |this, _, _, cx| {
                                this.port_forward.remove(&rid); cx.notify();
                            })))
                        .into_any_element()
                }).collect()
            }))
}