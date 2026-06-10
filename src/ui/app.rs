#![allow(unused)]
use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    h_flex,
    input::{Input, InputState},
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu,
        SidebarMenuItem, SidebarToggleButton,
    },
    v_flex, ActiveTheme, Icon, IconName, Root, Sizable,
};
use gpui_component::scroll::ScrollableElement as _;
use gpui_platform::application;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::pty::LocalPty;
use crate::ai::chat::{ChatState, ChatStatus};
use crate::ai::ui::chat_view::render_chat_view;
use crate::ai::ui::input_bar::{render_input_bar, InputMode};
use crate::ai::agent::AgentRunner;
use crate::ai::providers::openai::OpenAiProvider;

// ═══════════════════════════════════════════════════════════════════════════
// Navegación
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum Nav {
    #[default]
    Terminal,
    Termia,
    Git,
    Kanban,
    DevTools,
}

// ═══════════════════════════════════════════════════════════════════════════
// Estado de la app
// ═══════════════════════════════════════════════════════════════════════════

pub(crate) struct AppState {
    pub(crate) data_dir: PathBuf,
    pub(crate) nav: Nav,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) tabs: Vec<TabState>,
    pub(crate) active_tab: usize,
    pub(crate) status_message: String,
    /// Focus handle for terminal keyboard input.
    pub(crate) focus_handle: FocusHandle,
    /// Terminal font size in px (default 13, range 8-24).
    pub(crate) terminal_font_size: usize,
    /// Current terminal font family.
    pub(crate) terminal_font_family: String,
    // AI
    pub(crate) chat_state: ChatState,
    pub(crate) ai_mini_visible: bool,
    pub(crate) ai_api_key: String,
    pub(crate) ai_model: String,
    /// Selected agent index (0=OpenAI, 1=Anthropic, 2=Ollama).
    pub(crate) agent_index: usize,
    /// AI input entity for the chat input bar.
    pub(crate) ai_input: Entity<InputState>,
    /// AI input text (accumulated keystrokes).
    pub(crate) ai_text: String,
    /// Current input mode: Shell or AI.
    pub(crate) input_mode: InputMode,
    /// Command palette visibility (Ctrl+K).
    pub(crate) palette_visible: bool,
    /// Palette search text.
    pub(crate) palette_query: String,
}

#[derive(Clone)]
pub(crate) struct TabState {
    pub(crate) id: String,
    pub(crate) label: String,
    /// Terminal emulator — wrapped in Arc for sharing with PTY read task.
    pub(crate) terminal: std::sync::Arc<parking_lot::Mutex<crate::terminal::view::TerminalView>>,
    /// Local PTY process (spawned shell).
    pub(crate) pty: Option<std::sync::Arc<parking_lot::Mutex<LocalPty>>>,
}

impl TabState {
    fn new(id: String, label: String) -> Self {
        let term = crate::terminal::view::TerminalView::new(
            crate::terminal::view::TerminalSize::new(120, 40),
        );
        Self {
            id,
            label,
            terminal: std::sync::Arc::new(parking_lot::Mutex::new(term)),
            pty: None,
        }
    }
}

impl AppState {
    pub fn new(data_dir: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ai_input = cx.new(|cx| InputState::new(window, cx).placeholder("Ask Termia..."));
        let mut s = Self {
            data_dir,
            nav: Nav::Terminal,
            sidebar_collapsed: false,
            tabs: vec![],
            active_tab: 0,
            status_message: "Termia — lista".into(),
            focus_handle: cx.focus_handle(),
            terminal_font_size: 13,
            terminal_font_family: "monospace".into(),
            chat_state: ChatState::new(),
            ai_mini_visible: false,
            ai_api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            ai_model: std::env::var("TERMIA_MODEL").unwrap_or_else(|_| "gpt-4o".into()),
            agent_index: 0,
            ai_input,
            ai_text: String::new(),
            input_mode: InputMode::Ai,
            palette_visible: false,
            palette_query: String::new(),
        };
        // Abrir una pestaña de terminal local al iniciar
        s.new_terminal_tab(cx);
        s
    }

    /// Create a new local terminal tab with a shell PTY.
    pub(crate) fn new_terminal_tab(&mut self, cx: &mut Context<Self>) {
        let tab_id = Uuid::new_v4().to_string();
        let label = format!("term-{}", &tab_id[..6]);
        let tab = TabState::new(tab_id.clone(), label.clone());
        self.tabs.push(tab);
        let idx = self.tabs.len() - 1;
        self.active_tab = idx;
        self.nav = Nav::Terminal;
        self.status_message = format!("Nuevo terminal: {}", label);

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        let tab_id = self.tabs[idx].id.clone();
        let terminal = self.tabs[idx].terminal.clone();

        // Spawn PTY + read loop
        cx.spawn(async move |entity: gpui::WeakEntity<AppState>, cx| {
            let pty = match LocalPty::spawn(&shell) {
                Ok(p) => p,
                Err(e) => {
                    entity.update(cx, |this, cx| {
                        this.status_message = format!("Error PTY: {e}");
                        cx.notify();
                    }).ok();
                    return;
                }
            };

            let pty = std::sync::Arc::new(parking_lot::Mutex::new(pty));

            // Store PTY in tab
            entity.update(cx, |this, cx| {
                if let Some(tab) = this.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.pty = Some(pty.clone());
                }
                cx.notify();
            }).ok();

            // Read loop: poll PTY output → feed terminal emulator
            let term = terminal.clone();
            let pty2 = pty.clone();
            let entity2 = entity.clone();
            let tid = tab_id.clone();
            cx.spawn(async move |cx| {
                let mut buf = [0u8; 4096];
                loop {
                    // Check if tab still exists and PTY is alive
                    let keep_running = entity2.read_with(cx, |this, _| {
                        this.tabs.iter().any(|t| t.id == tid)
                    }).unwrap_or(false);
                    if !keep_running {
                        break;
                    }

                    let n = {
                        let mut p = pty2.lock();
                        match p.read_available(&mut buf) {
                            Ok(0) => {
                                if !p.is_alive() { break; }
                                drop(p);
                                // Yield before polling again
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                continue;
                            }
                            Ok(n) => n,
                            Err(_) => break,
                        }
                    };

                    // Feed to terminal emulator
                    term.lock().write(&buf[..n]);
                    entity2.update(cx, |_, cx| cx.notify()).ok();
                }
            }).detach();
        }).detach();
    }

    pub(crate) fn close_tab(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.tabs.len() {
            self.tabs.remove(idx);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.active_tab.saturating_sub(1);
            }
            self.status_message = if self.tabs.is_empty() {
                "Sin terminales".into()
            } else {
                format!("{} pestaña(s)", self.tabs.len())
            };
            cx.notify();
        }
    }

    pub(crate) fn send_ai_message(&mut self, message: String, cx: &mut Context<Self>) {
        if self.ai_api_key.is_empty() {
            self.chat_state.error = Some("OPENAI_API_KEY not set".into());
            cx.notify();
            return;
        }

        let api_key = self.ai_api_key.clone();
        let model = self.ai_model.clone();
        let data_dir = self.data_dir.clone();
        self.chat_state.status = ChatStatus::Streaming;
        cx.notify();

        cx.spawn(async move |entity: gpui::WeakEntity<AppState>, cx| {
            let result = TOKIO_RT.handle().block_on(async {
                let provider = std::sync::Arc::new(OpenAiProvider::new(api_key, None));
                let runner = AgentRunner::new(provider, model, data_dir);
                let state = entity.read_with(cx, |this, _| this.chat_state.clone()).ok().unwrap_or_default();
                runner.run(state, message).await
            });

            entity.update(cx, |this, cx| {
                match result {
                    Ok(state) => { this.chat_state = state; }
                    Err(e) => {
                        this.chat_state.status = ChatStatus::Error;
                        this.chat_state.error = Some(format!("{e}"));
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Render principal
// ═══════════════════════════════════════════════════════════════════════════

impl Render for AppState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::ui::views::*;
        let decorations = window.window_decorations();
        let is_csd = matches!(decorations, Decorations::Client { .. });
        let win_ctrls = window.window_controls();
        let is_maximized = window.is_maximized();

        let nav = self.nav;
        let collapsed = self.sidebar_collapsed;
        let ic = collapsed;

        let btn_fg       = cx.theme().foreground;
        let btn_hover    = cx.theme().secondary_hover;
        let btn_active   = cx.theme().secondary_active;
        let close_hover  = cx.theme().danger;
        let close_active = cx.theme().danger_active;
        let close_fg     = cx.theme().danger_foreground;

        v_flex().size_full().bg(cx.theme().background)
            .overflow_hidden()
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _window, cx| {
                let key = event.keystroke.key.as_str();
                let ctrl = event.keystroke.modifiers.control;
                
                if ctrl {
                    match key {
                        "k" => { this.palette_visible = !this.palette_visible; cx.notify(); }
                        "t" => { this.new_terminal_tab(cx); }
                        "w" => { if !this.tabs.is_empty() { this.close_tab(this.active_tab, cx); } }
                        _ => {
                            // Ctrl+1-9: switch to tab
                            if let Some(d) = key.chars().next().and_then(|c| c.to_digit(10)) {
                                if d > 0 && d as usize <= this.tabs.len() {
                                    this.active_tab = (d - 1) as usize;
                                    this.nav = Nav::Terminal;
                                    this.focus_handle.focus(_window, cx);
                                    cx.notify();
                                }
                            }
                        }
                    }
                }
                
                // Capture text input for AI chat when in AI mode
                if this.nav == Nav::Termia && !ctrl {
                    if key == "enter" || key == "return" {
                        let text = std::mem::take(&mut this.ai_text);
                        if !text.trim().is_empty() {
                            this.send_ai_message(text, cx);
                        }
                        cx.notify();
                    } else if key == "backspace" {
                        this.ai_text.pop();
                        cx.notify();
                    } else if key == "space" {
                        this.ai_text.push(' ');
                        cx.notify();
                    } else if key.len() == 1 {
                        this.ai_text.push_str(key);
                        cx.notify();
                    }
                }
            }))
            .map(|this| match decorations {
                Decorations::Client { tiling } => this
                    .when(!(tiling.top || tiling.left),    |d| d.rounded_tl(px(10.)))
                    .when(!(tiling.top || tiling.right),   |d| d.rounded_tr(px(10.)))
                    .when(!(tiling.bottom || tiling.left), |d| d.rounded_bl(px(10.)))
                    .when(!(tiling.bottom || tiling.right),|d| d.rounded_br(px(10.))),
                _ => this,
            })
            // ── Titlebar ──
            .child(
                h_flex()
                    .id("titlebar")
                    .w_full().h(px(34.)).flex_shrink_0()
                    .pl(px(12.))
                    .border_b_1().border_color(cx.theme().title_bar_border)
                    .bg(cx.theme().title_bar)
                    .window_control_area(WindowControlArea::Drag)
                    .child(h_flex().items_center().gap_2()
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default(); cx.stop_propagation();
                        })
                        .child(SidebarToggleButton::new().collapsed(ic)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar_collapsed = !this.sidebar_collapsed; cx.notify();
                            })))
                        .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("Termia")))
                    .child(div().flex_1().flex().items_center().justify_center()
                        .text_xs().text_color(cx.theme().muted_foreground)
                        .child(format!("terminal + IA · v{}", env!("CARGO_PKG_VERSION"))))
                    .child(h_flex()
                        .id("win-controls")
                        .items_center().flex_shrink_0().h_full()
                        .gap_1().pr_3()
                        .when(is_csd, |this| this
                            .when(win_ctrls.minimize, |this| this.child(
                                div().id("btn-min")
                                    .cursor_pointer().flex_shrink_0()
                                    .rounded_full().w_5().h_5()
                                    .flex().items_center().justify_center()
                                    .text_color(btn_fg)
                                    .hover(move |s| s.bg(btn_hover))
                                    .active(move |s| s.bg(btn_active))
                                    .on_mouse_down(MouseButton::Left, |_, w, cx| {
                                        w.prevent_default(); cx.stop_propagation();
                                    })
                                    .on_click(|_, w, cx| { cx.stop_propagation(); w.minimize_window(); })
                                    .child(Icon::new(IconName::WindowMinimize).small())
                            ))
                            .when(win_ctrls.maximize, |this| {
                                let icon = if is_maximized { IconName::WindowRestore } else { IconName::WindowMaximize };
                                this.child(
                                    div().id("btn-max")
                                        .cursor_pointer().flex_shrink_0()
                                        .rounded_full().w_5().h_5()
                                        .flex().items_center().justify_center()
                                        .text_color(btn_fg)
                                        .hover(move |s| s.bg(btn_hover))
                                        .active(move |s| s.bg(btn_active))
                                        .on_mouse_down(MouseButton::Left, |_, w, cx| {
                                            w.prevent_default(); cx.stop_propagation();
                                        })
                                        .on_click(|_, w, cx| { cx.stop_propagation(); w.zoom_window(); })
                                        .child(Icon::new(icon).small())
                                )
                            })
                            .child(
                                div().id("btn-close")
                                    .cursor_pointer().flex_shrink_0()
                                    .rounded_full().w_5().h_5()
                                    .flex().items_center().justify_center()
                                    .text_color(btn_fg)
                                    .hover(move |s| s.bg(close_hover).text_color(close_fg))
                                    .active(move |s| s.bg(close_active).text_color(close_fg))
                                    .on_mouse_down(MouseButton::Left, |_, w, cx| {
                                        w.prevent_default(); cx.stop_propagation();
                                    })
                                    .on_click(|_, w, cx| { cx.stop_propagation(); w.remove_window(); })
                                    .child(Icon::new(IconName::WindowClose).small())
                            )
                        )
                    )
            )
            // ── Body: Sidebar + Content ──
            .child(h_flex().flex_1().min_h_0()
                .child(Sidebar::new("main-sidebar").w(px(240.))
                    .collapsible(SidebarCollapsible::Icon).collapsed(collapsed)
                    .header(SidebarHeader::new()
                        .child(div().flex().items_center().justify_center().size_8().flex_shrink_0()
                            .rounded(cx.theme().radius).bg(cx.theme().sidebar_primary)
                            .text_color(cx.theme().sidebar_primary_foreground)
                            .child(Icon::new(IconName::SquareTerminal)))
                        .when(!ic, |this| this.child(v_flex().flex_1().overflow_hidden()
                            .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("Termia"))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Terminal + IA")))))
                    .child(SidebarGroup::new("Navigation").child(SidebarMenu::new()
                        .child(menuitem("Terminal", IconName::SquareTerminal, nav == Nav::Terminal, cx, |s, cx| { s.nav = Nav::Terminal; cx.notify(); }))
                        .child(menuitem("AI Chat", IconName::Search, nav == Nav::Termia, cx, |s, cx| { s.nav = Nav::Termia; cx.notify(); }))
                        .child(menuitem("Git", IconName::Globe, nav == Nav::Git, cx, |s, cx| { s.nav = Nav::Git; cx.notify(); }))
                        .child(menuitem("Kanban", IconName::LayoutDashboard, nav == Nav::Kanban, cx, |s, cx| { s.nav = Nav::Kanban; cx.notify(); }))
                        .child(menuitem("DevTools", IconName::Settings, nav == Nav::DevTools, cx, |s, cx| { s.nav = Nav::DevTools; cx.notify(); }))))
                    .footer(SidebarFooter::new().child(h_flex().gap_2()
                        .child(div().size_2().rounded_full().flex_shrink_0().bg(rgb(0x22c55e)))
                        .child(div().text_xs().text_color(cx.theme().muted_foreground)
                            .child(format!("{} tabs", self.tabs.len()))))))
                // ── Content area ──
                .child(v_flex().flex_1().h_full().min_w_0()
                    .when(!self.tabs.is_empty(), |d| d.child(terminal::render_tab_bar(self, cx)))
                    .child(render_content(self, cx))
                    .child(status_bar::render_status_bar(self, cx))))
            .when(self.palette_visible, |d| d.child(render_palette(self, cx)))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Contenido
// ═══════════════════════════════════════════════════════════════════════════

fn render_content(state: &AppState, cx: &mut Context<AppState>) -> AnyElement {
    use crate::ui::views::*;
    match state.nav {
        Nav::Terminal => terminal::render_terminal_area(state, cx).into_any_element(),
        Nav::Termia => render_termia_view(state, cx),
        Nav::Git => git::render_git_view(state, cx).into_any_element(),
        Nav::Kanban => kanban::render_kanban_view(state, cx).into_any_element(),
        Nav::DevTools => devtools::render_devtools_view(state, cx).into_any_element(),
    }
}

fn render_termia_view(state: &AppState, cx: &mut Context<AppState>) -> AnyElement {
    let theme = cx.theme().clone();
    let has_key = !state.ai_api_key.is_empty();
    let mode = state.input_mode;
    let agents = ["GPT-4o", "Claude", "Ollama"];
    let agent_idx = state.agent_index;
    v_flex().size_full().bg(theme.background)
        .child(
            h_flex().px_4().py_1().gap_1().border_b_1().border_color(theme.border)
                .child(div().text_xs().text_color(theme.muted_foreground).child("Model:"))
                .children(agents.iter().enumerate().map(|(i, name)| {
                    h_flex().px_2().py_1().rounded_sm()
                        .cursor_pointer()
                        .bg(if i == agent_idx { theme.primary } else { hsla(0.0, 0.0, 0.0, 0.0) })
                        .text_color(if i == agent_idx { theme.primary_foreground } else { theme.muted_foreground })
                        .text_xs()
                        .id(ElementId::Name(format!("agent-{i}").into()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.agent_index = i;
                            match i {
                                0 => this.ai_model = "gpt-4o".into(),
                                1 => this.ai_model = "claude-sonnet-4".into(),
                                _ => this.ai_model = "llama3".into(),
                            }
                            cx.notify();
                        }))
                        .child(*name)
                        .into_any_element()
                })))
        .child(render_chat_view(&state.chat_state, cx))
        .child(render_input_bar(mode, has_key, SharedString::from(state.ai_text.as_str()), cx))
        .into_any_element()
}

// ═══════════════════════════════════════════════════════════════════════════
// Sidebar helper
// ═══════════════════════════════════════════════════════════════════════════

fn menuitem(
    label: &str,
    icon: IconName,
    active: bool,
    cx: &mut Context<AppState>,
    on_click: impl Fn(&mut AppState, &mut Context<AppState>) + 'static,
) -> SidebarMenuItem {
    SidebarMenuItem::new(label)
        .icon(icon)
        .active(active)
        .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
}

// ═══════════════════════════════════════════════════════════════════════════
// Focusable
// ═══════════════════════════════════════════════════════════════════════════

impl gpui::Focusable for AppState {
    fn focus_handle(&self, _cx: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Command Palette
// ═══════════════════════════════════════════════════════════════════════════

fn render_palette(state: &mut AppState, cx: &mut Context<AppState>) -> AnyElement {
    let theme = cx.theme().clone();
    let commands: &[(&str, &str)] = &[
        ("New Terminal", "Ctrl+T"),
        ("AI Chat", "Switch to AI"),
        ("Git Panel", "Source control"),
        ("Kanban Board", "Task manager"),
        ("DevTools", "NVM & scripts"),
        ("Explain Terminal", "AI analyze output"),
        ("Close Palette", "Escape"),
    ];

    let filtered: Vec<(&str, &str)> = if state.palette_query.is_empty() {
        commands.iter().copied().collect()
    } else {
        let q = state.palette_query.to_lowercase();
        commands.iter().filter(|(name, _)| name.to_lowercase().contains(&q)).copied().collect()
    };

    div().absolute().inset_0().flex().items_center().justify_center()
        .bg(hsla(0.0, 0.0, 0.0, 0.5))
        .id("palette-overlay")
        .on_click(cx.listener(|this, _, _, cx| { this.palette_visible = false; cx.notify(); }))
        .child(
            v_flex().w(px(500.)).max_h(px(400.)).rounded_lg().bg(theme.background).border_1().border_color(theme.border).shadow_lg()
                .child(
                    div().px_4().py_2().border_b_1().border_color(theme.border)
                        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child("Command Palette")))
                .child(
                    div().flex_1().overflow_y_scrollbar().p_2().child(
                        v_flex().gap_0().children(filtered.iter().enumerate().map(|(i, &(name, desc))| {
                            let item_id = format!("palette-item-{i}");
                            h_flex().px_3().py_1p5().gap_3().items_center().rounded_md()
                                .hover(|s| s.bg(theme.primary))
                                .cursor_pointer()
                                .id(ElementId::Name(item_id.clone().into()))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    match name {
                                        "New Terminal" => { this.new_terminal_tab(cx); }
                                        "AI Chat" => { this.nav = Nav::Termia; }
                                        "Git Panel" => { this.nav = Nav::Git; }
                                        "Kanban Board" => { this.nav = Nav::Kanban; }
                                        "DevTools" => { this.nav = Nav::DevTools; }
                                        "Explain Terminal" => {
                                            let text = "Explain this terminal output".to_string();
                                            this.send_ai_message(text, cx);
                                            this.nav = Nav::Termia;
                                        }
                                        "Close Palette" => {}
                                        _ => {}
                                    }
                                    this.palette_visible = false;
                                    this.palette_query.clear();
                                    cx.notify();
                                }))
                                .child(div().text_sm().text_color(theme.foreground).child(name.to_string()))
                                .child(div().flex_1())
                                .child(div().text_xs().text_color(theme.muted_foreground).child(desc.to_string()))
                                .into_any_element()
                        }))
                    ))
        ).into_any_element()
}

// ═══════════════════════════════════════════════════════════════════════════
// Entry point
// ═══════════════════════════════════════════════════════════════════════════

/// Tokio runtime for async operations (AI providers, etc.).
static TOKIO_RT: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime")
});

pub fn run(data_dir: PathBuf) {
    TOKIO_RT.handle();
    let app = application().with_assets(crate::assets::Assets);
    app.run(move |cx: &mut App| {
        gpui_component::init(cx);
        crate::assets::load_themes(cx);
        let data_dir = data_dir.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(100.), px(100.)), size(px(1200.), px(800.))))),
                #[cfg(not(target_os = "linux"))]
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(9.0), px(9.0))),
                }),
                #[cfg(target_os = "linux")]
                window_background: WindowBackgroundAppearance::Transparent,
                #[cfg(target_os = "linux")]
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            move |window, cx| {
                let data_dir = data_dir.clone();
                let state = cx.new(|cx| AppState::new(data_dir, window, cx));
                cx.new(|cx| Root::new(state, window, cx))
            },
        ).unwrap();
    });
}
