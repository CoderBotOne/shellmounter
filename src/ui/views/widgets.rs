use gpui::prelude::*;
use gpui::*;
use gpui_component::{h_flex, v_flex, sidebar::SidebarMenuItem, ActiveTheme, Icon, Sizable, IconName};
use crate::ui::app::AppState;

const AC: [u32; 6] = [0xef4444, 0x6366f1, 0x22c55e, 0xa855f7, 0xf97316, 0x0ea5e9];


pub fn avatar_color(id: &str) -> u32 {
    AC[id.bytes().fold(0usize, |a, b| a.wrapping_add(b as usize)) % AC.len()]
}

pub fn menuitem(label: &str, icon: IconName, active: bool, cx: &mut Context<AppState>,
            f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static) -> SidebarMenuItem {
    SidebarMenuItem::new(label).icon(icon).active(active).on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
}

pub fn btn(label: &str, primary: bool, cx: &mut Context<AppState>,
       f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static) -> impl IntoElement {
    let id = format!("btn-{}", label.to_lowercase().replace(' ', "-"));
    let lbl = label.to_string();
    div().id(id).h_8().px_3()
        .rounded(cx.theme().radius).flex().items_center().gap_1().text_sm()
        .font_weight(FontWeight::MEDIUM).cursor_pointer()
        .when(primary, |d| d.bg(cx.theme().primary).text_color(cx.theme().primary_foreground)
              .hover(|d| d.bg(cx.theme().primary_hover)))
        .when(!primary, |d| d.bg(cx.theme().secondary).border_1().border_color(cx.theme().border)
              .hover(|d| d.bg(cx.theme().secondary_hover)))
        .child(lbl).on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
}

pub fn toggle(label: &str, active: bool, cx: &mut Context<AppState>,
          f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static) -> impl IntoElement {
    let lbl = label.to_string();
    div().id(format!("tgl-{}", label.to_lowercase())).flex_1().h_9().rounded(cx.theme().radius).flex().items_center().justify_center()
        .text_sm().font_weight(FontWeight::MEDIUM).cursor_pointer()
        .bg(if active { cx.theme().primary } else { cx.theme().secondary })
        .text_color(if active { cx.theme().primary_foreground } else { cx.theme().foreground })
        .child(lbl).on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
}

pub fn empty(title: &str, desc: &str, icon: IconName, cx: &mut Context<AppState>) -> impl IntoElement {
    let t = title.to_string();
    let d = desc.to_string();
    v_flex().size_full().items_center().justify_center().gap_2().pt_16()
        .child(Icon::new(icon).large())
        .child(div().font_weight(FontWeight::SEMIBOLD).text_base().child(t))
        .child(div().text_sm().text_color(cx.theme().muted_foreground).child(d))
}