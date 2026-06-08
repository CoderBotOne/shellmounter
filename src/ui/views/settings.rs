use gpui::prelude::*;
use gpui::*;
use gpui_component::{h_flex, v_flex, scroll::ScrollableElement as _, ActiveTheme, Theme, ThemeRegistry};
use gpui::FontWeight;
use crate::ui::app::AppState;


pub fn render_settings_view(_state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    use gpui_component::{Theme, ThemeRegistry};
    let current = Theme::global(cx).theme_name().clone();
    let themes: Vec<_> = ThemeRegistry::global(cx).sorted_themes().into_iter().cloned().collect();

    v_flex().flex_1().size_full().p_4().gap_2()
        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).mb_2().child("Themes"))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).mb_2()
            .child(format!("{} themes available", themes.len())))
        .child(div().flex_1().overflow_hidden()
            .child(v_flex().gap_1().overflow_y_scrollbar()
                .children(themes.iter().map(|theme| {
                    let is_active = theme.name.as_ref() == current.as_ref();
                    let name = theme.name.to_string();
                    let mode_label = if theme.mode.is_dark() { "dark" } else { "light" };
                    let mode_is_dark = theme.mode.is_dark();
                    let theme_clone = (*theme).clone();
                    let primary_bg = cx.theme().primary;
                    let primary_fg = cx.theme().primary_foreground;
                    let muted = cx.theme().muted_foreground;
                    let radius = cx.theme().radius;
                    h_flex().id(ElementId::Name(format!("theme-{}", name).into()))
                        .px_3().py_2().rounded(radius)
                        .gap_2().items_center().cursor_pointer()
                        .bg(if is_active { primary_bg } else { cx.theme().secondary })
                        .text_color(if is_active { primary_fg } else { cx.theme().foreground })
                        .hover(|d| d.bg(if is_active { primary_bg } else { cx.theme().secondary }))
                        .on_click(cx.listener(move |_this, _, _, cx| {
                            Theme::global_mut(cx).apply_config(&theme_clone);
                            cx.notify();
                        }))
                        .child(div().size_2().rounded_full().flex_shrink_0()
                            .bg(if is_active { primary_fg } else {
                                if mode_is_dark { hsla(0.664, 0.866, 0.5, 1.0) } else { hsla(0.123, 0.824, 0.427, 1.0) }
                            }))
                        .child(div().flex_1().text_sm().child(name))
                        .child(div().text_xs().text_color(if is_active { primary_fg } else { muted }).child(mode_label))
                }))))
}