use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    badge::Badge,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme,
};
use gpui_component::scroll::ScrollableElement as _;
use crate::kanban::{Board, Card, Priority};
use crate::ui::app::AppState;
use std::path::PathBuf;

pub fn render_kanban_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let path = state.data_dir.join("kanban.json");
    let board = Board::load(&path).unwrap_or_else(|_| Board::new("Termia Board"));

    v_flex().size_full().bg(theme.background)
        .child(
            // Header
            h_flex().px_4().py_2().gap_2().items_center().border_b_1().border_color(theme.border)
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child(board.name.clone()))
                .child(div().flex_1())
                .child(Button::new("kanban-add").primary().child("+ Card"))
        )
        .child(
            // Columns
            h_flex().flex_1().gap_3().p_4().overflow_y_scrollbar().children(
                board.columns.iter().map(|col| {
                    let card_count = col.cards.len();
                    v_flex().w(px(280.)).rounded_lg().border_1().border_color(theme.border).bg(theme.background)
                        .child(
                            h_flex().px_3().py_2().gap_2().items_center().border_b_1().border_color(theme.border)
                                .child(Badge::new().child(format!("{}", card_count)))
                                .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child(col.name.clone()))
                        )
                        .child(
                            div().flex_1().overflow_y_scrollbar().p_2().child(
                                v_flex().gap_2().children(
                                    col.cards.iter().map(|card| render_card(card, &theme).into_any_element()).collect::<Vec<AnyElement>>()
                                )
                            )
                        )
                        .into_any_element()
                }).collect::<Vec<AnyElement>>()
            )
        )
        .into_any_element()
}

fn render_card(card: &Card, theme: &gpui_component::Theme) -> impl IntoElement {
    let priority_color = hsla_from_hex(card.priority.color());
    v_flex().p_2().gap_1().rounded_md().border_1().border_color(theme.border).bg(theme.background).cursor_pointer().hover(|s| s.bg(theme.primary))
        .child(
            h_flex().gap_1().items_center()
                .child(div().size_1p5().rounded_full().bg(priority_color))
                .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child(card.title.clone()))
        )
        .child(
            h_flex().gap_1().flex_wrap().children(
                card.labels.iter().map(|l| Badge::new().child(l.clone()).into_any_element()).collect::<Vec<AnyElement>>()
            )
        )
        .child(
            h_flex().gap_2().items_center()
                .child(div().text_xs().text_color(theme.muted_foreground).child(
                    card.assignee.as_ref().map(|a| a.clone()).unwrap_or_else(|| "unassigned".to_string())
                ))
        )
}

fn hsla_from_hex(hex: u32) -> Hsla {
    let r = ((hex >> 16) & 0xff) as f32 / 255.0;
    let g = ((hex >> 8) & 0xff) as f32 / 255.0;
    let b = (hex & 0xff) as f32 / 255.0;
    hsla(r * 360.0, 0.7, 0.5, 1.0)
}
