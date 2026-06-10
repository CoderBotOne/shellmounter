#![allow(unused)]
use gpui::prelude::*;
use gpui::*;
use gpui_component::{badge::Badge, h_flex, v_flex, ActiveTheme};
use gpui_component::scroll::ScrollableElement as _;

use crate::ai::chat::{ChatState, ChatStatus, Message, MessagePart, Role, ToolState};

/// Render the AI chat conversation view.
pub fn render_chat_view(state: &ChatState, cx: &mut Context<crate::ui::app::AppState>) -> impl IntoElement {
    let theme = cx.theme().clone();

    if state.messages.is_empty() {
        return v_flex().flex_1().items_center().justify_center().gap_2()
            .child(div().text_xl().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child("Ask Termia anything"))
            .child(div().text_sm().text_color(theme.muted_foreground).child("Explain command output, fix errors, generate snippets, or run a full task."))
            .into_any_element();
    }

    let messages: Vec<AnyElement> = state.messages.iter().map(|msg| {
        let is_streaming = state.status == ChatStatus::Streaming
            && Some(msg.id.as_str()) == state.last_message().map(|m| m.id.as_str());
        render_msg(msg, is_streaming, &theme)
    }).collect();

    v_flex().size_full().gap_3().p_3().overflow_y_scrollbar()
        .child(v_flex().gap_3().children(messages))
        .into_any_element()
}

fn render_msg(msg: &Message, _streaming: bool, theme: &gpui_component::Theme) -> AnyElement {
    match msg.role {
        Role::User => {
            let text: SharedString = extract_text(&msg.parts).into();
            v_flex().gap_2()
                .child(h_flex().gap_2().items_center()
                    .child(Badge::new().child("You"))
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child("You"))
                )
                .child(div().p_3().rounded_md().bg(theme.background).border_1().border_color(theme.border)
                    .child(div().text_sm().text_color(theme.foreground).child(text)))
                .into_any_element()
        }
        Role::Assistant => {
            let parts: Vec<AnyElement> = msg.parts.iter().map(|p| render_part(p, theme)).collect();
            v_flex().gap_2()
                .child(h_flex().gap_2().items_center()
                    .child(Badge::new().child("AI"))
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child("Termia"))
                )
                .child(v_flex().gap_3().children(parts))
                .into_any_element()
        }
        Role::System => div().into_any_element(),
    }
}

fn render_part(part: &MessagePart, theme: &gpui_component::Theme) -> AnyElement {
    match part {
        MessagePart::Text { text } => {
            let t: SharedString = text.clone().into();
            div().text_sm().text_color(theme.foreground).child(t).into_any_element()
        }
        MessagePart::Reasoning { text } => {
            let t: SharedString = text.clone().into();
            div().p_2().rounded_md().border_1().border_color(theme.border).bg(theme.background)
                .child(v_flex().gap_1()
                    .child(div().text_xs().text_color(theme.muted_foreground).child("Reasoning"))
                    .child(div().text_xs().text_color(theme.foreground).child(t))
                )
                .into_any_element()
        }
        _ => render_tool(part, theme),
    }
}

fn render_tool(part: &MessagePart, theme: &gpui_component::Theme) -> AnyElement {
    let (icon, label, detail, state) = match part {
        MessagePart::ToolBash { state, input, .. } => (
            "$", "Bash", input.as_ref().map(|i| i.command.clone()).unwrap_or_default(), *state
        ),
        MessagePart::ToolReadFile { state, input, .. } => (
            "R", "Read", input.as_ref().map(|i| i.path.clone()).unwrap_or_default(), *state
        ),
        MessagePart::ToolWriteFile { state, input, .. } => (
            "W", "Write", input.as_ref().map(|i| i.path.clone()).unwrap_or_default(), *state
        ),
        MessagePart::ToolEdit { state, input, .. } => (
            "E", "Edit", input.as_ref().map(|i| i.path.clone()).unwrap_or_default(), *state
        ),
        MessagePart::ToolGlob { state, input, .. } => (
            "*", "Glob", input.as_ref().map(|i| i.pattern.clone()).unwrap_or_default(), *state
        ),
        MessagePart::ToolGrep { state, input, .. } => (
            "~", "Grep", input.as_ref().map(|i| i.pattern.clone()).unwrap_or_default(), *state
        ),
        _ => ("?", "Tool", String::new(), ToolState::Pending),
    };

    let status_color = match state {
        ToolState::Pending | ToolState::Running => theme.muted_foreground,
        ToolState::OutputAvailable | ToolState::Done | ToolState::Approved => theme.primary,
        ToolState::Error | ToolState::Rejected => hsla(0.0, 0.84, 0.60, 1.0),
        ToolState::ApprovalRequested => hsla(45.0, 0.93, 0.47, 1.0),
    };

    let status_text = match state {
        ToolState::Pending => "Pending...",
        ToolState::Running => "Running...",
        ToolState::OutputAvailable | ToolState::Done => "Done",
        ToolState::Error => "Error",
        ToolState::ApprovalRequested => "Approval needed",
        ToolState::Approved => "Approved",
        ToolState::Rejected => "Rejected",
    };

    let detail_s: SharedString = detail.into();
    div().p_2().rounded_md().border_1().border_color(theme.border).bg(theme.background)
        .child(v_flex().gap_1()
            .child(h_flex().gap_2().items_center()
                .child(Badge::new().child(icon))
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
                .child(div().text_xs().text_color(status_color).child(status_text))
            )
            .child(div().text_xs().text_color(theme.muted_foreground).child(detail_s))
        )
        .into_any_element()
}

fn extract_text(parts: &[MessagePart]) -> String {
    parts.iter()
        .filter_map(|p| if let MessagePart::Text { text } = p { Some(text.as_str()) } else { None })
        .collect::<Vec<_>>()
        .join("\n")
}
