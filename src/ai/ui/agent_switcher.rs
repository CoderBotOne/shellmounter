#![allow(unused)]
#![allow(dead_code)]
use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
    ActiveTheme,
};

/// Agent/model selector using Button + dropdown.
pub struct AgentSwitcher {
    pub agents: Vec<AgentEntry>,
    pub selected_index: usize,
    pub open: bool,
}

#[derive(Clone)]
pub struct AgentEntry {
    pub name: String,
    pub model: String,
}

impl AgentSwitcher {
    pub fn new() -> Self {
        Self {
            agents: vec![
                AgentEntry { name: "Termia (Claude)".into(), model: "claude-sonnet-4".into() },
                AgentEntry { name: "Termia (GPT-4o)".into(), model: "gpt-4o".into() },
                AgentEntry { name: "Local (Ollama)".into(), model: "llama3".into() },
            ],
            selected_index: 0,
            open: false,
        }
    }
}

impl Render for AgentSwitcher {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = &self.agents[self.selected_index];
        let label = SharedString::from(format!("{} v", selected.name));

        div().relative().child(
            Button::new("agent-switcher-trigger")
                .ghost()
                .child(label)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open = !this.open;
                    cx.notify();
                }))
        )
        .into_any_element()
    }
}
