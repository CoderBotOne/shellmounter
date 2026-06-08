use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    h_flex, v_flex, scroll::ScrollableElement as _, ActiveTheme, Icon, Sizable, IconName,
    button::{Button, ButtonVariants as _},
};
use gpui::FontWeight;
use crate::ui::app::{AppState, Modal, Nav, HostViewMode};
use crate::db::hosts::Host;
use super::widgets::{empty, avatar_color, status_dot};

pub fn render_hosts_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let query: String = state.search_query.clone().into();
    let query_lower = query.to_lowercase();
    let view_mode = state.host_view_mode;

    v_flex().flex_1().size_full()
        // ── Header ──
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(Button::new("add-host").primary().child("+ Nuevo host")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.modal = Some(Modal::HostEditor); cx.notify();
                })))
            .child(h_flex().flex_1().h_8().px_3().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
                .bg(cx.theme().secondary).items_center().gap_1())
            .child(div().flex_1())
            // View mode toggles
            .child(render_view_toggle(state, cx)))
        // ── Content ──
        .child(match view_mode {
            HostViewMode::Grid => render_host_grid(state, cx, &query_lower).into_any_element(),
            HostViewMode::List => render_host_list(state, cx, &query_lower).into_any_element(),
        })
}

/// Grid / List toggle buttons.
fn render_view_toggle(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let mode = state.host_view_mode;
    h_flex().gap_1().pr_2()
        .child(
            Button::new("view-grid")
                .when(mode == HostViewMode::Grid, |b| b.primary())
                .when(mode != HostViewMode::Grid, |b| b.ghost())
                .child(Icon::new(IconName::LayoutDashboard).size_4())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.host_view_mode = HostViewMode::Grid;
                    cx.notify();
                }))
        )
        .child(
            Button::new("view-list")
                .when(mode == HostViewMode::List, |b| b.primary())
                .when(mode != HostViewMode::List, |b| b.ghost())
                .child(Icon::new(IconName::Menu).size_4())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.host_view_mode = HostViewMode::List;
                    cx.notify();
                }))
        )
}

/// Grid layout: card grid with flexible columns.
fn render_host_grid(state: &AppState, cx: &mut Context<AppState>, query_lower: &str) -> impl IntoElement {
    let mut cards: Vec<AnyElement> = vec![];
    for (_gn, hosts) in &state.groups {
        let filtered: Vec<&Host> = if query_lower.is_empty() {
            hosts.iter().collect()
        } else {
            hosts.iter().filter(|h| {
                h.label.to_lowercase().contains(query_lower)
                    || h.hostname.to_lowercase().contains(query_lower)
                    || h.username.to_lowercase().contains(query_lower)
            }).collect()
        };
        for h in filtered {
            cards.push(render_host_grid_card(h, state, cx).into_any_element());
        }
    }

    if cards.is_empty() {
        return empty("Sin hosts", "Agrega tu primer servidor SSH.", IconName::Network, cx).into_any_element();
    }

    div().flex_1().overflow_y_scrollbar().p_4()
        .flex().flex_wrap().gap_3()
        .children(cards)
        .into_any_element()
}

/// Single grid card.
fn render_host_grid_card(host: &Host, state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let hid = host.id.clone();
    let hid_edit = host.id.clone();
    let hid_del = host.id.clone();
    let lbl = host.label.clone();
    let ac = avatar_color(&host.id);
    let first: SharedString = lbl.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into()).into();
    let conn = state.tabs.iter().any(|t| t.connected);

    v_flex().id(format!("grid-card-{}", hid)).w(px(200.)).px_4().py_3().rounded(cx.theme().radius).gap_3()
        .bg(cx.theme().background).border_1().border_color(cx.theme().border)
        .cursor_pointer().hover(|d| d.bg(cx.theme().accent))
        .on_click(cx.listener(move |this, _, _, cx| { this.connect_host(&hid, cx); }))
        // Avatar + status
        .child(h_flex().items_center().gap_2()
            .child(
                div().size_10().rounded(cx.theme().radius).flex().items_center().justify_center()
                    .flex_shrink_0().bg(rgb(ac)).text_color(rgb(0xffffff))
                    .font_weight(FontWeight::BOLD).text_sm().child(first)
            )
            .child(div().flex_1())
            .when(conn, |d| d.child(status_dot(true))))
        // Name
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground)
            .child(lbl.clone()))
        // Connection info
        .child(div().text_xs().text_color(cx.theme().muted_foreground)
            .child(format!("{}@{}:{}", host.username, host.hostname, host.port)))
        // Actions
        .child(h_flex().gap_1()
            .child(Button::new(format!("edit-{}", hid_edit)).ghost()
                .child("Editar")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.host_form.editing_id = Some(hid_edit.clone());
                    this.modal = Some(Modal::HostEditor);
                    cx.notify();
                })))
            .child(Button::new(format!("del-{}", hid_del)).ghost().danger()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.modal = Some(Modal::ConfirmDelete(hid_del.clone()));
                    cx.notify();
                }))
                .child("Eliminar")))
}

/// List layout: grouped cards (current design).
fn render_host_list(state: &AppState, cx: &mut Context<AppState>, query_lower: &str) -> impl IntoElement {
    div().id("host-scroll").flex_1().overflow_y_scrollbar().p_4()
        .children({
            let mut items: Vec<AnyElement> = vec![];
            if state.hosts.is_empty() {
                items.push(empty("Sin hosts", "Agrega tu primer servidor SSH.", IconName::Network, cx).into_any_element());
            } else {
                for (gn, hosts) in &state.groups {
                    let filtered: Vec<&Host> = if query_lower.is_empty() {
                        hosts.iter().collect()
                    } else {
                        hosts.iter().filter(|h| {
                            h.label.to_lowercase().contains(query_lower)
                                || h.hostname.to_lowercase().contains(query_lower)
                                || h.username.to_lowercase().contains(query_lower)
                        }).collect()
                    };
                    if filtered.is_empty() { continue; }
                    items.push(v_flex().mb_4().gap_1()
                        .child(div().px_1().mb_1().text_xs().font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().muted_foreground).child(gn.clone()))
                        .children(filtered.iter().map(|h| render_host_card(h, state, cx)))
                        .into_any_element());
                }
            }
            items
        })
}

pub fn render_host_card(host: &Host, state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let hid = host.id.clone(); let hid2 = host.id.clone();
    let hid_edit = host.id.clone(); let hid_del = host.id.clone();
    let lbl = host.label.clone();
    let sel = state.selected_host_id.as_deref() == Some(&host.id);
    let conn = state.tabs.iter().any(|t| t.connected);
    let ac = avatar_color(&host.id);
    let first: SharedString = lbl.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into()).into();

    h_flex().id(format!("host-card-{}", hid)).w_full().px_3().py_2().rounded(cx.theme().radius).gap_3()
        .bg(cx.theme().background).border_1()
        .border_color(if sel { cx.theme().primary } else { cx.theme().border })
        .cursor_pointer().hover(|d| d.bg(cx.theme().accent))
        .on_click(cx.listener(move |this, _, _, cx| { this.connect_host(&hid, cx); }))
        .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
            this.selected_host_id = Some(hid2.clone());
            cx.notify();
        }))
        .child(div().size_9().rounded(cx.theme().radius).flex().items_center().justify_center()
            .flex_shrink_0().bg(rgb(ac)).text_color(rgb(0xffffff)).font_weight(FontWeight::BOLD).text_sm().child(first))
        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(lbl))
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(format!("ssh  {}@{}:{}", host.username, host.hostname, host.port))))
        .when(conn, |d| d.child(status_dot(true)))
        .child(h_flex().gap_1().ml_2()
            .child(Button::new(format!("edit-host-{}", hid_edit)).ghost()
                .child("Editar")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.host_form.editing_id = Some(hid_edit.clone());
                    this.modal = Some(Modal::HostEditor);
                    cx.notify();
                })))
            .child(Button::new(format!("del-host-{}", hid_del)).ghost().danger()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.modal = Some(Modal::ConfirmDelete(hid_del.clone()));
                    cx.notify();
                }))
                .child("Eliminar")))
}
