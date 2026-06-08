use gpui::prelude::*;
use gpui::*;
use gpui_component::{h_flex, v_flex, scroll::ScrollableElement as _, input::Input, ActiveTheme, Icon, Sizable, IconName};
use gpui::FontWeight;
use uuid::Uuid;
use crate::ui::app::{AppState, Modal, HostForm, KeyGenForm};
use crate::db::hosts::AuthMethod;
use crate::ssh::keys::SshKey;
use crate::vault::store::SecretKind;
use super::widgets::{btn, toggle, avatar_color};


pub fn render_modal(state: &AppState, cx: &mut Context<AppState>, modal: &Modal) -> impl IntoElement {
    let (title, body): (String, AnyElement) = match modal {
        Modal::HostEditor => ("Nuevo Host".into(), render_host_form(state, cx).into_any_element()),
        Modal::KeyGen => ("Nueva Key SSH".into(), render_key_gen_form(state, cx).into_any_element()),
        Modal::VaultUnlock => ("Desbloquear Vault".into(), render_vault_unlock_form(state, cx).into_any_element()),
        Modal::ConfirmDelete(ref id) => {
            let lbl = state.hosts.iter().find(|h| &h.id == id).map(|h| h.label.clone()).unwrap_or_default();
            ("Eliminar Host".into(), render_confirm_delete(id.clone(), &lbl, cx).into_any_element())
        }
    };

    div().absolute().inset_0().flex().items_center().justify_center().bg(gpui::rgba(0x00000066))
        .child(v_flex().w(px(460.)).rounded_xl().bg(cx.theme().background).border_1().border_color(cx.theme().border).p_6().gap_4()
            .child(h_flex().items_center().justify_between()
                .child(div().font_weight(FontWeight::SEMIBOLD).text_base().child(title))
                .child(div().id("btn-close-modal").size_6().rounded(cx.theme().radius).flex().items_center().justify_center()
                    .bg(cx.theme().secondary).cursor_pointer().hover(|d| d.bg(cx.theme().secondary_hover))
                    .on_click(cx.listener(|this, _, _, cx| { this.modal = None; cx.notify(); }))
                    .child(Icon::new(IconName::Close).small())))
            .child(body))
}

pub fn render_host_form(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().gap_3()
        .child(Input::new(&state.host_form.label))
        .child(Input::new(&state.host_form.hostname))
        .child(h_flex().gap_3()
            .child(div().flex_1().child(Input::new(&state.host_form.username)))
            .child(div().w(px(80.)).child(Input::new(&state.host_form.port))))
        .child(Input::new(&state.host_form.group))
        // Auth selector
        .child(v_flex().gap_1p5()
            .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child("Autenticación"))
            .child(h_flex().gap_2()
                .child(toggle("SSH Key", &state.host_form.auth_type == "key", cx, |s, cx| { s.host_form.auth_type = "key".into(); cx.notify(); }))
                .child(toggle("Password", &state.host_form.auth_type == "password", cx, |s, cx| { s.host_form.auth_type = "password".into(); cx.notify(); }))
                .child(toggle("Agent", &state.host_form.auth_type == "agent", cx, |s, cx| { s.host_form.auth_type = "agent".into(); cx.notify(); }))))
        // Key picker
        .when(&state.host_form.auth_type == "key", |d| {
            d.child(v_flex().gap_1p5()
                .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child("SSH Key"))
                .child(div().h_9().px_3().rounded(cx.theme().radius).bg(cx.theme().background).border_1().border_color(cx.theme().border)
                    .flex().items_center().text_sm().text_color(cx.theme().muted_foreground)
                    .child(if state.available_keys.is_empty() {
                        "Sin keys — genera una en Keychain".to_string()
                    } else if let Some(ref kid) = state.host_form.selected_key_id {
                        state.available_keys.iter().find(|k| &k.fingerprint == kid).map(|k| k.label.clone()).unwrap_or_else(|| "Seleccionar...".into())
                    } else { "Seleccionar...".to_string() }))
                .when(!state.available_keys.is_empty(), |d| d.child(
                    v_flex().gap_0p5().max_h(px(160.)).overflow_y_scrollbar().border_1().border_color(cx.theme().border)
                        .rounded(cx.theme().radius)
                        .children(state.available_keys.iter().map(|k| {
                            let fp = k.fingerprint.clone();
                            let sel = state.host_form.selected_key_id.as_deref() == Some(&fp);
                            h_flex().id(format!("key-{}", &fp[..8])).px_3().py_2().gap_2().cursor_pointer()
                                .bg(if sel { cx.theme().accent } else { cx.theme().background })
                                .hover(|d| d.bg(cx.theme().accent))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.host_form.selected_key_id = Some(fp.clone()); cx.notify();
                                }))
                                .child(Icon::new(IconName::HardDrive).small())
                                .child(v_flex().gap_0p5()
                                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(k.label.clone()))
                                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(k.fingerprint[..20.min(k.fingerprint.len())].to_string())))
                                .into_any_element()
                        }))
                )))
        })
        .when(state.host_form.auth_type == "password", |d| {
            d.child(Input::new(&state.host_form.password))
        })
        .child(h_flex().gap_2().justify_end().pt_1()
            .child(btn("Cancelar", false, cx, |s, cx| { s.modal = None; cx.notify(); }))
            .child(btn("Guardar", true, cx, |s, cx| { s.save_host(cx); })))
}

pub fn render_key_gen_form(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().gap_3()
        .child(Input::new(&state.key_gen_form.label))
        .child(h_flex().gap_2()
            .child(toggle("Ed25519", &state.key_gen_form.key_type == "ed25519", cx, |s, cx| { s.key_gen_form.key_type = "ed25519".into(); cx.notify(); }))
            .child(toggle("ECDSA P-256", &state.key_gen_form.key_type == "ecdsa-p256", cx, |s, cx| { s.key_gen_form.key_type = "ecdsa-p256".into(); cx.notify(); })))
        .child(Input::new(&state.key_gen_form.passphrase))
        .child(h_flex().gap_2().justify_end().pt_1()
            .child(btn("Cancelar", false, cx, |s, cx| { s.modal = None; cx.notify(); }))
            .child(btn("Generar", true, cx, |s, cx| { s.generate_key(cx); })))
}

pub fn render_vault_unlock_form(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().gap_3()
        .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Ingresa la contraseña del vault para desbloquear tus keys SSH."))
        .child(Input::new(&state.vault_password))
        .child(h_flex().gap_2().justify_end().pt_1()
            .child(btn("Cancelar", false, cx, |s, cx| { s.modal = None; cx.notify(); }))
            .child(btn("Desbloquear", true, cx, |s, cx| { s.unlock_vault(cx); })))
}

pub fn render_confirm_delete(id: String, label: &str, cx: &mut Context<AppState>) -> impl IntoElement {
    let id2 = id.clone();
    v_flex().gap_3()
        .child(div().text_sm().text_color(cx.theme().muted_foreground)
            .child(format!("¿Eliminar \"{}\" permanentemente?", label)))
        .child(h_flex().gap_2().justify_end().pt_1()
            .child(btn("Cancelar", false, cx, |s, cx| { s.modal = None; cx.notify(); }))
            .child(div().id("btn-delete-confirm").px_4().py_2().rounded(cx.theme().radius).bg(rgb(0xef4444)).text_color(rgb(0xffffff))
                .text_sm().font_weight(FontWeight::MEDIUM).cursor_pointer().hover(|d| d.bg(rgb(0xdc2626)))
                .on_click(cx.listener(move |this, _, _, cx| { this.delete_host(&id2, cx); }))
                .child("Eliminar")))
}