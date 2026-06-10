#![allow(unused)]
#![allow(dead_code)]
use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    badge::Badge,
    h_flex, v_flex,
    ActiveTheme,
};

/// Agent task list using Badge for task items.
pub struct TodoStrip {
    pub tasks: Vec<TodoItem>,
    pub expanded: bool,
}

#[derive(Clone)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TodoStatus { Pending, InProgress, Completed, Cancelled }

impl TodoStrip {
    pub fn new() -> Self { Self { tasks: Vec::new(), expanded: false } }
}

impl Render for TodoStrip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        if self.tasks.is_empty() {
            return div().into_any_element();
        }

        let pending: usize = self.tasks.iter()
            .filter(|t| matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress))
            .count();
        let completed: usize = self.tasks.iter()
            .filter(|t| t.status == TodoStatus::Completed)
            .count();
        let summary = SharedString::from(format!("Tasks: {} pending / {} done", pending, completed));

        div().border_t_1().border_color(theme.border)
            .child(
                div().px_3().py_1().cursor_pointer().hover(|s| s.bg(theme.primary))
                    .child(h_flex().gap_2().items_center()
                        .child(Badge::new().child(summary))
                        .child(div().text_xs().text_color(theme.muted_foreground).child(" v"))
                    ),
            )
            .into_any_element()
    }
}
