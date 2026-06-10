use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    h_flex, v_flex, scroll::ScrollableElement as _,
    input::Input,
    button::{Button, ButtonVariants as _},
    switch::Switch,
    ActiveTheme, Icon, Sizable, IconName,
};
use uuid::Uuid;
use crate::ui::app::AppState;
use crate::ssh::port_forward::{ForwardKind, PortForwardRule};
use super::widgets::empty;

pub fn render_port_forward_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let rules = state.port_forward.list().to_vec();
    let rules_len = rules.len();
    let rules_empty = rules.is_empty();

    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("Port Forwarding"))
            .child(div().flex_1())
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(format!("{} reglas", rules_len))))
        .child(h_flex().h_10().px_4().gap_2().border_b_1().border_color(cx.theme().border).items_center()
            .child(div().w(px(100.)).child(Input::new(&state.pf_label)))
            .child(render_kind_selector(state, cx))
            .child(div().w(px(80.)).child(Input::new(&state.pf_local_port)))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child("→"))
            .child(div().w(px(100.)).child(Input::new(&state.pf_remote_host)))
            .child(div().w(px(60.)).child(Input::new(&state.pf_remote_port)))
            .child(Button::new("add-pf").primary()
                .child(Icon::new(IconName::Plus).size_4())
                .on_click(cx.listener(|this, _, _, cx| {
                    let label = this.pf_label.read(cx).value();
                    let local: u16 = this.pf_local_port.read(cx).value().parse().unwrap_or(8080);
                    let remote_host = this.pf_remote_host.read(cx).value();
                    let remote: u16 = this.pf_remote_port.read(cx).value().parse().unwrap_or(80);
                    let kind = match this.pf_kind.as_str() {
                        "Remote" => ForwardKind::Remote,
                        "Dynamic" => ForwardKind::Dynamic,
                        _ => ForwardKind::Local,
                    };
                    let lbl = if label.is_empty() { format!("port-{}", local) } else { label.to_string() };
                    let rh = if remote_host.is_empty() { "localhost".to_string() } else { remote_host.to_string() };
                    let rule = PortForwardRule {
                        id: Uuid::new_v4().to_string(),
                        label: lbl,
                        kind,
                        local_port: local,
                        remote_host: rh,
                        remote_port: remote,
                        enabled: false,
                    };
                    this.port_forward.add(rule);
                    this.status_message = "Regla agregada".into();
                    cx.notify();
                }))))
        .child(div().flex_1().overflow_y_scrollbar().p_4()
            .children(if rules_empty {
                vec![empty("Sin reglas", "Crea túneles SSH con port forwarding.", IconName::Network, cx).into_any_element()]
            } else {
                rules.iter().map(|r| render_pf_rule(r, cx)).collect()
            }))
}

fn render_pf_rule(r: &PortForwardRule, cx: &mut Context<AppState>) -> AnyElement {
    let rid = r.id.clone();
    let rid_toggle = r.id.clone();
    let label = r.label.clone();
    let kind_display = r.kind.display().to_string();
    let describe = r.describe();
    let enabled = r.enabled;

    h_flex().w_full().px_3().py_2().rounded(cx.theme().radius).gap_3()
        .bg(cx.theme().background).border_1().border_color(cx.theme().border).mb_1()
        .child(div().size_2().rounded_full().flex_shrink_0().mt_1()
            .bg(rgb(if enabled { 0x22c55e } else { 0x6e7681 })))
        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
            .child(h_flex().gap_2().items_center()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(label))
                .child(div().text_xs().px_1p5().py_0p5().rounded(cx.theme().radius)
                    .bg(cx.theme().secondary).text_color(cx.theme().muted_foreground).child(kind_display)))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).font_family("monospace").child(describe)))
        .child(Switch::new(format!("toggle-{}", rid_toggle)).checked(enabled)
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(rule) = this.port_forward.list_mut().iter_mut().find(|x| x.id == rid_toggle) {
                    rule.enabled = !rule.enabled;
                    this.status_message = if rule.enabled {
                        format!("Túnel {} activado", rule.label)
                    } else {
                        format!("Túnel {} desactivado", rule.label)
                    };
                }
                cx.notify();
            })))
        .child(Button::new(format!("del-{}", rid)).ghost()
            .child(Icon::new(IconName::Close).size_3().text_color(rgb(0xef4444)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.port_forward.remove(&rid);
                cx.notify();
            })))
        .into_any_element()
}

fn render_kind_selector(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let kinds = ["Local", "Remote", "Dynamic"];
    h_flex().gap_1().children(kinds.iter().map(|k| {
        let is_active = state.pf_kind == *k;
        let k2 = k.to_string();
        Button::new(format!("pf-kind-{}", k))
            .when(is_active, |b| b.primary())
            .when(!is_active, |b| b.ghost())
            .child(k2.clone())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.pf_kind = k2.clone();
                cx.notify();
            }))
    }))
}
