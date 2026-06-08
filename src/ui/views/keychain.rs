use gpui::prelude::*;
use gpui::*;
use gpui_component::{h_flex, v_flex, scroll::ScrollableElement as _, ActiveTheme, Icon, Sizable, IconName};
use gpui::FontWeight;
use crate::ui::app::{AppState, Modal};
use crate::ssh::keys::SshKey;
use super::widgets::{btn, empty, avatar_color};


pub fn render_keychain_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let vok = state.vault_unlocked;
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(btn("+ Nueva Key", vok, cx, |s, cx| { s.modal = Some(Modal::KeyGen); cx.notify(); }))
            .child(btn("Importar Key", vok, cx, |s, cx| {
                if let Some(home) = dirs::home_dir() {
                    s.import_key(&home.join(".ssh").join("id_ed25519"), cx);
                }
            }))
            .child(div().flex_1())
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{} keys", state.available_keys.len()))))
        .child(div().flex_1().overflow_y_scrollbar().p_4()
            .children(if !vok {
                vec![empty("Vault bloqueado", "Desbloquea el vault para ver tus keys.", IconName::HardDrive, cx).into_any_element()]
            } else if state.available_keys.is_empty() {
                vec![empty("Sin keys", "Genera una nueva key SSH.", IconName::HardDrive, cx).into_any_element()]
            } else {
                state.available_keys.iter().map(|k| render_key_card(k, cx)).collect()
            }))
}

pub fn render_key_card(key: &SshKey, cx: &mut Context<AppState>) -> AnyElement {
    let ac = avatar_color(&key.fingerprint);
    let first: SharedString = key.label.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "K".into()).into();
    h_flex().w_full().px_3().py_2().rounded(cx.theme().radius).gap_3().bg(cx.theme().background)
        .border_1().border_color(cx.theme().border).mb_1()
        .child(div().size_9().rounded(cx.theme().radius).flex().items_center().justify_center()
            .flex_shrink_0().bg(rgb(ac)).text_color(rgb(0xffffff)).font_weight(FontWeight::BOLD).text_xs().child(first))
        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(key.label.clone()))
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(format!("{} — {}...", key.key_type.display_name(), &key.fingerprint[..16.min(key.fingerprint.len())]))))
        .into_any_element()
}