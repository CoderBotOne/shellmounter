use gpui::prelude::*;
use gpui::*;
use gpui_component::{h_flex, v_flex, scroll::ScrollableElement as _, ActiveTheme, Icon, Sizable, IconName};
use uuid::Uuid;
use crate::ui::app::AppState;
use crate::ssh::snippets::Snippet;
use super::widgets::{btn, empty};


pub fn render_snippets_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let snippets = state.snippet_store.as_ref().and_then(|s| s.list().ok()).unwrap_or_default();
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(btn("+ Nuevo", true, cx, |s, cx| {
                let snip = Snippet { id: Uuid::new_v4().to_string(), label: "nuevo".into(),
                    command: "echo hello".into(), description: "".into(), tags: vec![],
                    created_at: 0, updated_at: 0 };
                s.save_snippet(&snip, cx);
            }))
            .child(div().flex_1())
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{} snippets", snippets.len()))))
        .child(div().flex_1().overflow_y_scrollbar().p_4()
            .children(if snippets.is_empty() {
                vec![empty("Sin snippets", "Guarda comandos frecuentes.", IconName::SquareTerminal, cx).into_any_element()]
            } else {
                snippets.iter().map(|s| {
                    let cmd = s.command.clone();
                    h_flex().id(format!("snip-{}", s.id)).w_full().px_3().py_2().rounded(cx.theme().radius).gap_3().bg(cx.theme().background)
                        .border_1().border_color(cx.theme().border).mb_1().cursor_pointer().hover(|d| d.bg(cx.theme().accent))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            // Send command to active terminal if connected
                            if let Some(tab) = this.tabs.get(this.active_tab) {
                                if let Some(ref sess) = tab.session {
                                    let sess = sess.clone();
                                    let cmd_with_newline = format!("{}\r", cmd);
                                    let data = cmd_with_newline.into_bytes();
                                    cx.spawn(async move |_entity: gpui::WeakEntity<AppState>, _cx| {
                                        let mut s = sess.lock();
                                        let _ = s.send(&data).await;
                                    }).detach();
                                    // Also write to the terminal view for visual feedback
                                    tab.terminal.lock().write(cmd.as_bytes());
                                    tab.terminal.lock().write(b"\r\n");
                                    this.status_message = format!("Sent: {}", &cmd);
                                } else {
                                    this.status_message = "No active terminal connection".into();
                                }
                            }
                            cx.notify();
                        }))
                        .child(Icon::new(IconName::SquareTerminal).small())
                        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(s.label.clone()))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).font_family("monospace").child(s.command.clone())))
                        .into_any_element()
                }).collect()
            }))
}