use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    h_flex, v_flex, scroll::ScrollableElement as _,
    input::Input,
    button::{Button, ButtonVariants as _},
    ActiveTheme, Icon, Sizable, IconName,
};
use uuid::Uuid;
use gpui::FontWeight;
use crate::ui::app::AppState;
use crate::ssh::snippets::Snippet;
use super::widgets::empty;

pub fn render_snippets_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let snippets = state.snippet_store.as_ref().and_then(|s| s.list().ok()).unwrap_or_default();
    v_flex().flex_1().size_full()
        // ── Header ──
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("Snippets"))
            .child(div().flex_1())
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(format!("{} snippets", snippets.len()))))
        // ── Form ──
        .child(h_flex().h_10().px_4().gap_2().border_b_1().border_color(cx.theme().border).items_center()
            .child(
                div().w(px(160.)).child(Input::new(&state.snippet_label))
            )
            .child(
                div().flex_1().child(Input::new(&state.snippet_command))
            )
            .child(
                Button::new("add-snippet").primary()
                    .child(Icon::new(IconName::Plus).size_4())
                    .on_click(cx.listener(|this, _, _, cx| {
                        let label = this.snippet_label.read(cx).value();
                        let command = this.snippet_command.read(cx).value();
                        if !label.is_empty() && !command.is_empty() {
                            let snip = Snippet {
                                id: Uuid::new_v4().to_string(),
                                label: label.to_string(),
                                command: command.to_string(),
                                description: String::new(),
                                tags: vec![],
                                created_at: 0,
                                updated_at: 0,
                            };
                            this.save_snippet(&snip, cx);
                            // Clear fields
                        }
                    }))
            ))
        // ── List ──
        .child(div().flex_1().overflow_y_scrollbar().p_4()
            .children(if snippets.is_empty() {
                vec![empty("Sin snippets", "Guarda comandos frecuentes para enviar al terminal.", IconName::SquareTerminal, cx).into_any_element()]
            } else {
                snippets.iter().map(|s| {
                    let cmd = s.command.clone();
                    let sid = s.id.clone();
                    h_flex().id(format!("snip-{}", s.id)).w_full().px_3().py_2().rounded(cx.theme().radius).gap_3()
                        .bg(cx.theme().background).border_1().border_color(cx.theme().border).mb_1()
                        // Click area: send to terminal
                        .cursor_pointer().hover(|d| d.bg(cx.theme().accent))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(tab) = this.tabs.get(this.active_tab) {
                                if let Some(ref sess) = tab.session {
                                    let sess = sess.clone();
                                    let cmd_with_newline = format!("{}\r", cmd);
                                    let data = cmd_with_newline.into_bytes();
                                    cx.spawn(async move |_entity: gpui::WeakEntity<AppState>, _cx| {
                                        let mut s = sess.lock();
                                        let _ = s.send(&data).await;
                                    }).detach();
                                    tab.terminal.lock().write(cmd.as_bytes());
                                    tab.terminal.lock().write(b"\r\n");
                                    this.status_message = format!("Sent: {}", &cmd);
                                } else {
                                    this.status_message = "Sin conexión activa".into();
                                }
                            }
                            cx.notify();
                        }))
                        .child(Icon::new(IconName::SquareTerminal).small().text_color(cx.theme().muted_foreground))
                        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(s.label.clone()))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).font_family("monospace").child(s.command.clone())))
                        // Delete button
                        .child(
                            Button::new(format!("del-snip-{}", sid)).ghost()
                                .child(Icon::new(IconName::Close).size_3().text_color(rgb(0xef4444)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.delete_snippet(&sid, cx);
                                }))
                        )
                        .into_any_element()
                }).collect()
            }))
}
