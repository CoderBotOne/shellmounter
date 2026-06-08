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
            .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().muted_foreground).child("Tamaño de fuente del terminal"))
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
                    }))
                )))

        // Font family
        .child(v_flex().gap_1()
            .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().muted_foreground).child("Fuente mono"))
            .child(h_flex().gap_2().flex_wrap()
                .child(font_btn("JetBrains Mono", cx))
                .child(font_btn("Fira Code", cx))
                .child(font_btn("Cascadia Code", cx))
                .child(font_btn("Monospace", cx))))
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

fn font_btn(name: &str, cx: &mut Context<AppState>) -> impl IntoElement {
    let n = name.to_string();
    Button::new(format!("font-{}", name.to_lowercase().replace(' ', "-")))
        .ghost()
        .child(n)
        .on_click(cx.listener(move |_this, _, _, cx| {
            cx.notify();
        }))
}
