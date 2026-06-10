use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    h_flex, v_flex, scroll::ScrollableElement as _,
    button::{Button, ButtonVariants as _},
    ActiveTheme, Theme, ThemeRegistry, Sizable,
};
use gpui::FontWeight;
use crate::ui::app::AppState;

pub fn render_settings_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let current = Theme::global(cx).theme_name().clone();
    let themes: Vec<_> = ThemeRegistry::global(cx).sorted_themes().into_iter().cloned().collect();
    let fonts = ["JetBrains Mono", "Fira Code", "Cascadia Code", "monospace"];

    v_flex().flex_1().size_full().p_6().gap_4().overflow_y_scrollbar()
        // ── Appearance ──
        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).mb_2().child("Apariencia"))

        // Theme selector
        .child(v_flex().gap_1()
            .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().muted_foreground).child("Tema"))
            .child(h_flex().gap_2().flex_wrap()
                .children(themes.iter().enumerate().map(|(i, theme)| {
                    let is_active = theme.name.as_ref() == current.as_ref();
                    let name = theme.name.to_string();
                    let theme_clone = (*theme).clone();
                    Button::new(format!("theme-{}", i))
                        .when(is_active, |b| b.primary())
                        .when(!is_active, |b| b.secondary())
                        .child(name.clone())
                        .on_click(cx.listener(move |_this, _, _, cx| {
                            Theme::global_mut(cx).apply_config(&theme_clone);
                            cx.notify();
                        }))
                }))))

        // Font size
        .child(v_flex().gap_1()
            .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().muted_foreground)
                .child("Tamaño de fuente del terminal"))
            .child(h_flex().gap_3().items_center()
                .child(div().text_sm().text_color(cx.theme().foreground)
                    .child(format!("{} px", state.terminal_font_size)))
                .child(h_flex().gap_1()
                    .child(font_size_btn("-", state.terminal_font_size > 8, cx, |s, cx| {
                        s.terminal_font_size = (s.terminal_font_size - 1).max(8);
                        cx.notify();
                    }))
                    .child(font_size_btn("+", state.terminal_font_size < 24, cx, |s, cx| {
                        s.terminal_font_size = (s.terminal_font_size + 1).min(24);
                        cx.notify();
                    })))))

        // Font family
        .child(v_flex().gap_1()
            .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().muted_foreground)
                .child("Fuente mono"))
            .child(h_flex().gap_2().flex_wrap()
                .children(fonts.iter().map(|f| {
                    let active = state.terminal_font_family == *f;
                    let f2 = f.to_string();
                    Button::new(format!("font-{}", f.to_lowercase().replace(' ', "-")))
                        .when(active, |b| b.primary())
                        .when(!active, |b| b.ghost())
                        .child(f2.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.terminal_font_family = f2.clone();
                            cx.notify();
                        }))
                }))))

        // ── Import/Export ──
        .child(div().h_4()).child(div().border_t_1().border_color(cx.theme().border).flex_1())
        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Importar / Exportar"))

        .child(v_flex().gap_2()
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child("Exporta tu configuración (hosts, snippets, temas) o importa desde un archivo JSON."))
            .child(h_flex().gap_2()
                .child({
                    Button::new("export-config").secondary()
                        .child("Exportar configuración")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let path = this.data_dir.join("shellmounter-export.json");
                            let export = serde_json::json!({
                                "version": env!("CARGO_PKG_VERSION"),
                                "hosts_count": this.hosts.len(),
                                "terminal_font_size": this.terminal_font_size,
                                "terminal_font_family": this.terminal_font_family,
                            });
                            match std::fs::write(&path, serde_json::to_string_pretty(&export).unwrap_or_default()) {
                                Ok(()) => this.status_message = format!("Exportado a {}", path.display()),
                                Err(e) => this.status_message = format!("Error: {e}"),
                            }
                            cx.notify();
                        }))
                })
                .child({
                    Button::new("import-config").ghost()
                        .child("Importar configuración")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let path = this.data_dir.join("shellmounter-export.json");
                            if path.exists() {
                                match std::fs::read_to_string(&path) {
                                    Ok(json) => {
                                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json) {
                                            if let Some(fs) = val.get("terminal_font_size").and_then(|v| v.as_u64()) {
                                                this.terminal_font_size = fs as usize;
                                            }
                                            if let Some(ff) = val.get("terminal_font_family").and_then(|v| v.as_str()) {
                                                this.terminal_font_family = ff.to_string();
                                            }
                                            this.status_message = "Configuración importada".into();
                                        }
                                    }
                                    Err(e) => this.status_message = format!("Error: {e}"),
                                }
                            } else {
                                this.status_message = "No se encontró archivo de exportación".into();
                            }
                            cx.notify();
                        }))
                })))
}

fn font_size_btn(
    label: &str,
    enabled: bool,
    cx: &mut Context<AppState>,
    f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static,
) -> impl IntoElement {
    let lbl = label.to_string();
    Button::new(format!("fs-{}", label))
        .when(enabled, |b| b.secondary())
        .when(!enabled, |b| b.ghost())
        .child(lbl)
        .on_click(cx.listener(move |this, _, _, cx| {
            if enabled { f(this, cx); }
        }))
}
