use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    badge::Badge,
    button::{Button, ButtonVariants as _},
    input::Input,
    h_flex, v_flex, ActiveTheme,
};
use gpui_component::scroll::ScrollableElement as _;
use crate::git;
use crate::ui::app::AppState;
use std::path::PathBuf;

/// Render the Git source control panel.
pub fn render_git_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let theme = cx.theme().clone();

    let repo_path = git::find_repo(&state.data_dir).unwrap_or(state.data_dir.clone());
    let git_data = match repo_path.exists() {
        true => {
            let status = git::get_status(&repo_path).ok();
            let log = git::log(&repo_path, 20).ok();
            let branches = git::branches(&repo_path).ok();
            (status, log, branches)
        }
        false => (None, None, None),
    };

    v_flex().size_full().bg(theme.background)
        .child(
            // Header with branch info
            h_flex().px_4().py_2().gap_2().items_center().border_b_1().border_color(theme.border)
                .child(Badge::new().child(
                    git_data.0.as_ref().map(|s| s.branch.clone()).unwrap_or_else(|| "no repo".to_string())
                ))
                .child(div().text_xs().text_color(theme.muted_foreground).child(
                    format!("{} files", git_data.0.as_ref().map(|s| s.files.len()).unwrap_or(0))
                ))
                .child(div().flex_1())
                .child(Button::new("git-stage-all").ghost().child("Stage all"))
                .child(Button::new("git-commit").primary().child("Commit"))
        )
        .child(
            // File list
            div().flex_1().overflow_y_scrollbar().child(
                v_flex().gap_0().children(
                    git_data.0.as_ref().map(|status| {
                        status.files.iter().map(|f| {
                            let color = hsla_color(f.status.color());
                            let bg = theme.background;
                            h_flex().px_4().py_1().gap_2().items_center().hover(|s| s.bg(theme.primary))
                                .child(div().text_xs().text_color(color).child(f.status.label()))
                                .child(div().text_sm().text_color(theme.foreground).child(f.path.clone()))
                                .child(div().flex_1())
                                .child(Badge::new().child(if f.staged { "staged" } else { "modified" }))
                                .into_any_element()
                        }).collect::<Vec<AnyElement>>()
                    }).unwrap_or_default()
                )
            )
        )
        .child(
            // Commit log
            v_flex().border_t_1().border_color(theme.border)
                .child(div().px_4().py_1().text_xs().text_color(theme.muted_foreground).child("Recent commits"))
                .child(
                    div().flex_1().overflow_y_scrollbar().max_h(px(200.)).child(
                        v_flex().gap_0().children(
                            git_data.1.as_ref().map(|log| {
                                log.iter().map(|c| {
                                    h_flex().px_4().py_1().gap_2().items_center()
                                        .child(div().text_xs().text_color(theme.muted_foreground).font_family("monospace").child(c.short_oid.clone()))
                                        .child(div().text_sm().text_color(theme.foreground).child(c.message.clone()))
                                        .child(div().flex_1())
                                        .child(div().text_xs().text_color(theme.muted_foreground).child(c.author.clone()))
                                        .into_any_element()
                                }).collect::<Vec<AnyElement>>()
                            }).unwrap_or_default()
                        )
                    )
                )
        )
        .into_any_element()
}

fn hsla_color(hex: u32) -> Hsla {
    let r = ((hex >> 16) & 0xff) as f32 / 255.0;
    let g = ((hex >> 8) & 0xff) as f32 / 255.0;
    let b = (hex & 0xff) as f32 / 255.0;
    hsla(r * 360.0, 0.7, 0.5, 1.0)
}
