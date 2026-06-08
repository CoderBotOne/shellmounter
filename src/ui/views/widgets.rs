use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
    sidebar::SidebarMenuItem,
    switch::Switch,
    badge::Badge,
    ActiveTheme, Icon, IconName, Sizable,
};
use crate::ui::app::AppState;

const AC: [u32; 6] = [0xef4444, 0x6366f1, 0x22c55e, 0xa855f7, 0xf97316, 0x0ea5e9];

pub fn avatar_color(id: &str) -> u32 {
    AC[id.bytes().fold(0usize, |a, b| a.wrapping_add(b as usize)) % AC.len()]
}

pub fn menuitem(label: &str, icon: IconName, active: bool, cx: &mut Context<AppState>,
                f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static) -> SidebarMenuItem {
    SidebarMenuItem::new(label).icon(icon).active(active).on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
}

/// Button wrapper using gpui-component Button.
pub fn btn(label: &str, primary: bool, cx: &mut Context<AppState>,
           f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static) -> impl IntoElement {
    let id = ElementId::Name(format!("btn-{}", label.to_lowercase().replace(' ', "-")).into());
    let button = Button::new(id).child(label.to_string());
    let button = if primary { button.primary() } else { button.secondary() };
    button.on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
}

/// Toggle wrapper using gpui-component Switch.
pub fn toggle(label: &str, active: bool, cx: &mut Context<AppState>,
              f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static) -> impl IntoElement {
    let id = ElementId::Name(format!("toggle-{}", label.to_lowercase().replace(' ', "-")).into());
    Switch::new(id).checked(active).label(label)
        .on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
}

/// Empty state placeholder.
pub fn empty(title: &str, desc: &str, icon: IconName, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().size_full().items_center().justify_center().gap_2()
        .child(Icon::new(icon).size_8().text_color(cx.theme().muted_foreground))
        .child(div().font_weight(FontWeight::SEMIBOLD).text_lg().text_color(cx.theme().foreground).child(title.to_string()))
        .child(div().text_sm().text_color(cx.theme().muted_foreground).child(desc.to_string()))
}

/// Small status dot using gpui-component Badge.
pub fn status_dot(active: bool) -> impl IntoElement {
    let color = if active { gpui::rgb(0x22c55e) } else { gpui::rgb(0xef4444) };
    div().size_2().rounded_full().flex_shrink_0().bg(color)
}
