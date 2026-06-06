//! ShellMounter — main application shell.
//!
//! Layout: sidebar (host tree) | main area (terminal tabs + SFTP panel)
//! State: selected host, open tabs, vault unlock status, theme.

use gpui::*;
use gpui_component::{
    button::Button,
    input::{TextInput, TextInputVariant},
    tabs::{Tab, TabBar, TabBarVariant},
    sidebar::Sidebar,
    tree::{Tree, TreeNode, TreeVariant},
    dialog::Dialog,
};
use std::path::PathBuf;
use std::sync::Arc;

use crate::db::hosts::{AuthMethod, Host, HostDb};
use crate::ssh::session::SshSession;
use crate::vault::store::Vault;

/// A terminal tab in the tab bar.
#[derive(Clone)]
struct TerminalTab {
    id: String,
    host_label: String,
    connected: bool,
}

/// Global application state.
pub struct AppState {
    // ── Data ─────────────────────────────────────────────────────────
    host_db: Arc<HostDb>,
    vault: Arc<parking_lot::Mutex<Vault>>,
    data_dir: PathBuf,

    // ── UI state ─────────────────────────────────────────────────────
    /// All open terminal tabs
    tabs: Vec<TerminalTab>,
    /// Index of the active tab
    active_tab: usize,
    /// SSH sessions (one per tab)
    sessions: Vec<Option<SshSession>>,
    /// Selected host in the tree
    selected_host_id: Option<String>,
    /// Hosts list (refreshed from DB)
    hosts: Vec<Host>,
    /// Host groups for the tree
    groups: Vec<(String, Vec<Host>)>,
    /// Vault unlocked flag
    vault_unlocked: bool,
    /// Show vault unlock dialog
    show_vault_dialog: bool,
    /// Show host editor dialog
    show_host_editor: bool,
    /// SFTP panel visible
    show_sftp: bool,
    /// Status bar message
    status_message: String,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let host_db = Arc::new(HostDb::open(&data_dir).expect("Failed to open host database"));
        let vault = Arc::new(parking_lot::Mutex::new(
            Vault::open(&data_dir).expect("Failed to open vault"),
        ));

        // Load hosts from DB
        let hosts = host_db.list_hosts(None).unwrap_or_default();
        let groups = Self::group_hosts(&hosts, &host_db);

        let vault_unlocked = vault.lock().is_unlocked();

        Self {
            host_db,
            vault,
            data_dir,
            tabs: vec![],
            active_tab: 0,
            sessions: vec![],
            selected_host_id: None,
            hosts,
            groups,
            vault_unlocked,
            show_vault_dialog: !vault_unlocked,
            show_host_editor: false,
            show_sftp: false,
            status_message: "Ready".into(),
        }
    }

    /// Group hosts by their group_name field.
    fn group_hosts(hosts: &[Host], _db: &HostDb) -> Vec<(String, Vec<Host>)> {
        let mut groups: std::collections::BTreeMap<String, Vec<Host>> =
            std::collections::BTreeMap::new();

        // Ungrouped first
        let ungrouped: Vec<_> = hosts.iter().filter(|h| h.group_name.is_none()).cloned().collect();
        if !ungrouped.is_empty() {
            groups.insert("Ungrouped".into(), ungrouped);
        }

        for host in hosts {
            if let Some(ref group) = host.group_name {
                groups.entry(group.clone()).or_default().push(host.clone());
            }
        }

        groups.into_iter().collect()
    }

    /// Open a new terminal tab for a host.
    fn open_host(&mut self, host_id: &str, cx: &mut Context<Self>) {
        // Check if already open
        if let Some(pos) = self.tabs.iter().position(|t| t.id == host_id) {
            self.active_tab = pos;
            cx.notify();
            return;
        }

        if let Some(host) = self.hosts.iter().find(|h| h.id == host_id) {
            let tab = TerminalTab {
                id: host.id.clone(),
                host_label: host.label.clone(),
                connected: false,
            };

            self.tabs.push(tab);
            self.sessions.push(None);
            self.active_tab = self.tabs.len() - 1;
            self.selected_host_id = Some(host_id.to_string());
            self.status_message = format!("Connecting to {}...", host.label);

            // Spawn SSH connection in background
            let host_clone = host.clone();
            let tab_idx = self.tabs.len() - 1;
            cx.spawn(|this, mut cx| async move {
                let result = SshSession::connect(
                    &host_clone.hostname,
                    host_clone.port,
                    &host_clone.username,
                    "~/.ssh/id_ed25519", // TODO: get from vault
                )
                .await;

                this.update(&mut cx, |state, cx| {
                    match result {
                        Ok(session) => {
                            state.sessions[tab_idx] = Some(session);
                            if let Some(tab) = state.tabs.get_mut(tab_idx) {
                                tab.connected = true;
                            }
                            state.status_message =
                                format!("Connected to {}", host_clone.label);
                        }
                        Err(e) => {
                            state.status_message =
                                format!("Failed: {}", e);
                        }
                    }
                    cx.notify();
                })
                .ok();
            });

            cx.notify();
        }
    }

    /// Close a terminal tab.
    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
            self.sessions.remove(index);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.active_tab.saturating_sub(1);
            }
            cx.notify();
        }
    }

    /// Refresh host list from database.
    fn refresh_hosts(&mut self) {
        if let Ok(hosts) = self.host_db.list_hosts(None) {
            self.groups = Self::group_hosts(&hosts, &self.host_db);
            self.hosts = hosts;
        }
    }
}

// ── GPUI Render ─────────────────────────────────────────────────────────

impl Render for AppState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = div()
            .size_full()
            .flex_col()
            .bg(rgb(0x0a0e14))
            .font_family("Inter, -apple-system, sans-serif");

        // ── Vault unlock dialog ──────────────────────────────────────
        if self.show_vault_dialog {
            return view.child(self.render_vault_dialog(cx));
        }

        // ── Host editor dialog ───────────────────────────────────────
        if self.show_host_editor {
            return view.child(self.render_host_editor(cx));
        }

        // ── Main layout ──────────────────────────────────────────────
        view.child(
            div()
                .size_full()
                .flex()
                // ── Sidebar ───────────────────────────────────────────
                .child(self.render_sidebar(cx))
                // ── Main content ──────────────────────────────────────
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .flex_col()
                        // Tab bar
                        .child(self.render_tab_bar(cx))
                        // Terminal / SFTP area
                        .child(self.render_main_area(cx))
                        // Status bar
                        .child(self.render_status_bar()),
                ),
        )
    }
}

impl AppState {
    /// Render the vault unlock dialog.
    fn render_vault_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x0a0e14))
            .child(
                div()
                    .w(px(400.))
                    .p_6()
                    .bg(rgb(0x141929))
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(0x1e2538))
                    .flex_col()
                    .gap_4()
                    .child(Label::new("Unlock Vault").size(px(20.)).weight(FontWeight::BOLD).color(rgb(0xe1e5ee)))
                    .child(Label::new("Enter your master password to unlock SSH keys and credentials.").color(rgb(0x8d91a5)))
                    .child(
                        TextInput::new(cx, "••••••••")
                            .variant(TextInputVariant::Password)
                    )
                    .child(
                        div().flex().gap_2().justify_end().child(
                            Button::new(cx, "Unlock")
                                .on_click(cx.listener(|this, _: &ClickEvent, cx| {
                                    this.show_vault_dialog = false;
                                    this.vault_unlocked = true;
                                    cx.notify();
                                }))
                        ),
                    ),
            )
    }

    /// Render the host editor dialog.
    fn render_host_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x000000, 0.6))
            .child(
                div()
                    .w(px(500.))
                    .p_6()
                    .bg(rgb(0x141929))
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(0x1e2538))
                    .flex_col()
                    .gap_3()
                    .child(Label::new("New Host").size(px(18.)).weight(FontWeight::BOLD).color(rgb(0xe1e5ee)))
                    .child(Label::new("Label").size(px(12.)).color(rgb(0x8d91a5)))
                    .child(TextInput::new(cx, "").placeholder("Production DB"))
                    .child(Label::new("Hostname").size(px(12.)).color(rgb(0x8d91a5)))
                    .child(TextInput::new(cx, "").placeholder("10.0.1.50"))
                    .child(Label::new("Username").size(px(12.)).color(rgb(0x8d91a5)))
                    .child(TextInput::new(cx, "").placeholder("root"))
                    .child(Label::new("Port").size(px(12.)).color(rgb(0x8d91a5)))
                    .child(TextInput::new(cx, "").placeholder("22"))
                    .child(
                        div().flex().gap_2().justify_end().children(vec![
                            Button::new(cx, "Cancel")
                                .on_click(cx.listener(|this, _: &ClickEvent, cx| {
                                    this.show_host_editor = false;
                                    cx.notify();
                                })),
                            Button::new(cx, "Save")
                                .on_click(cx.listener(|this, _: &ClickEvent, cx| {
                                    this.show_host_editor = false;
                                    cx.notify();
                                })),
                        ]),
                    ),
            )
    }

    /// Render the sidebar with host tree.
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(260.))
            .h_full()
            .bg(rgb(0x0d1117))
            .border_r_1()
            .border_color(rgb(0x1e2538))
            .flex_col()
            .child(
                // Header
                div()
                    .h(px(40.))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(0x1e2538))
                    .child(Label::new("Hosts").size(px(13.)).weight(FontWeight::BOLD).color(rgb(0xe1e5ee)))
                    .child(
                        Button::new(cx, "+")
                            .on_click(cx.listener(|this, _: &ClickEvent, cx| {
                                this.show_host_editor = true;
                                cx.notify();
                            }))
                    ),
            )
            .child(
                // Host tree
                div().flex_1().overflow_y_scroll().children(
                    self.groups.iter().map(|(group_name, hosts)| {
                        div().flex_col().child(
                            // Group header
                            div()
                                .h(px(28.))
                                .px_3()
                                .flex()
                                .items_center()
                                .child(Label::new(group_name.clone()).size(px(11.)).color(rgb(0x5a5f73)).weight(FontWeight::BOLD))
                        )
                        .children(hosts.iter().map(|host| {
                            let is_selected = self.selected_host_id.as_deref() == Some(&host.id);
                            let host_id = host.id.clone();

                            div()
                                .h(px(32.))
                                .px_3()
                                .pl_6()
                                .flex()
                                .items_center()
                                .gap_2()
                                .when(is_selected, |d| d.bg(rgb(0x1a2744)))
                                .hover(|d| d.bg(rgb(0x141d2e)))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _: &ClickEvent, cx| {
                                    let id = host_id.clone();
                                    this.open_host(&id, cx);
                                }))
                                .child(
                                    // Status dot
                                    div()
                                        .w(px(6.))
                                        .h(px(6.))
                                        .rounded_full()
                                        .bg(if self.tabs.iter().any(|t| t.id == host.id && t.connected) {
                                            rgb(0x22c55e) // green = connected
                                        } else {
                                            rgb(0x3b3f54) // gray = offline
                                        })
                                )
                                .child(Label::new(host.label.clone()).size(px(13.)).color(if is_selected { rgb(0xe1e5ee) } else { rgb(0x8d91a5) }))
                                .child(div().flex_1())
                                .child(Label::new(host.hostname.clone()).size(px(10.)).color(rgb(0x4a4f62)))
                        }))
                    })
                )
            )
    }

    /// Render the tab bar.
    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(px(36.))
            .bg(rgb(0x0d1117))
            .border_b_1()
            .border_color(rgb(0x1e2538))
            .flex()
            .overflow_x_scroll()
            .children(
                self.tabs.iter().enumerate().map(|(i, tab)| {
                    let is_active = i == self.active_tab;
                    let tab_idx = i;

                    div()
                        .h_full()
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .border_r_1()
                        .border_color(rgb(0x1e2538))
                        .when(is_active, |d| d.bg(rgb(0x0a0e14)).border_b_2().border_color(rgb(0x3b82f6)))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, cx| {
                            this.active_tab = tab_idx;
                            cx.notify();
                        }))
                        .child(
                            // Connection status indicator
                            div()
                                .w(px(8.))
                                .h(px(8.))
                                .rounded_full()
                                .bg(if tab.connected {
                                    rgb(0x22c55e)
                                } else {
                                    rgb(0xeab308) // yellow = connecting
                                })
                        )
                        .child(Label::new(tab.host_label.clone()).size(px(12.)).color(rgb(0xe1e5ee)))
                        .child(
                            // Close button
                            div()
                                .w(px(20.))
                                .h(px(20.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_md()
                                .hover(|d| d.bg(rgb(0x1e2538)))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _: &ClickEvent, cx| {
                                    this.close_tab(tab_idx, cx);
                                }))
                                .child(Label::new("×").size(px(14.)).color(rgb(0x6b7280)))
                        )
                })
            )
            .child(div().flex_1()) // Spacer
            .child(
                // SFTP toggle button
                div()
                    .h_full()
                    .px_3()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, cx| {
                        this.show_sftp = !this.show_sftp;
                        cx.notify();
                    }))
                    .child(Label::new("SFTP").size(px(11.)).color(rgb(0x8d91a5)))
            )
    }

    /// Render the main content area (terminal or welcome screen).
    fn render_main_area(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        if self.tabs.is_empty() {
            // Welcome screen
            div()
                .flex_1()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .flex_col()
                .gap_3()
                .bg(rgb(0x0a0e14))
                .child(Label::new("ShellMounter").size(px(24.)).weight(FontWeight::BOLD).color(rgb(0xe1e5ee)))
                .child(Label::new("Select a host from the sidebar or add a new one").size(px(14.)).color(rgb(0x6b7280)))
                .child(
                    div().flex().gap_2().mt_4().child(
                        Button::new(_cx, "New Host")
                            .on_click(_cx.listener(|this, _: &ClickEvent, cx| {
                                this.show_host_editor = true;
                                cx.notify();
                            }))
                    ),
                )
        } else if self.show_sftp {
            // SFTP panel
            div()
                .flex_1()
                .size_full()
                .flex_col()
                .bg(rgb(0x0a0e14))
                .child(div().flex_1().child(Label::new("SFTP Browser — coming soon").color(rgb(0x8d91a5))))
        } else {
            // Terminal area
            let tab = &self.tabs[self.active_tab];
            if tab.connected {
                div()
                    .flex_1()
                    .size_full()
                    .bg(rgb(0x0a0e14))
                    .child(Label::new(format!("Connected to {}", tab.host_label)).color(rgb(0x8d91a5)))
            } else {
                div()
                    .flex_1()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(0x0a0e14))
                    .child(Label::new(format!("Connecting to {}...", tab.host_label)).color(rgb(0xeab308)))
            }
        }
    }

    /// Render the status bar at the bottom.
    fn render_status_bar(&self) -> impl IntoElement {
        div()
            .h(px(24.))
            .bg(rgb(0x0d1117))
            .border_t_1()
            .border_color(rgb(0x1e2538))
            .px_3()
            .flex()
            .items_center()
            .child(Label::new(self.status_message.clone()).size(px(11.)).color(rgb(0x4a4f62)))
            .child(div().flex_1())
            .child(
                Label::new(if self.vault_unlocked { "🔓 vault" } else { "🔒 vault" })
                    .size(px(11.))
                    .color(rgb(0x4a4f62)),
            )
    }
}

/// Launch the GPUI application.
pub fn run(data_dir: PathBuf) {
    App::new().run(|cx: &mut AppContext| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Point::new(100., 100.),
                    Size::new(1200., 800.),
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("ShellMounter".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| cx.new(|_cx| AppState::new(data_dir.clone())),
        )
        .unwrap();
    });
}
