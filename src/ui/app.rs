// ShellMounter — main application shell.
// Full flow: unlock vault → add hosts → connect via SSH → manage files.

use gpui::prelude::*;
use gpui::*;
use gpui_platform::application;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::hosts::{AuthMethod, Host, HostDb};
use crate::vault::store::Vault;

// ── Helpers ──────────────────────────────────────────────────────────────

/// Inline styled text element — avoids gpui-component version conflict.
fn text_el(
    content: impl Into<SharedString>,
    size: Pixels,
    color: impl Into<Hsla>,
) -> impl IntoElement {
    div()
        .text_size(size)
        .text_color(color)
        .child(content.into())
}

fn text_bold(
    content: impl Into<SharedString>,
    size: Pixels,
    color: impl Into<Hsla>,
) -> impl IntoElement {
    div()
        .text_size(size)
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .child(content.into())
}

// ── State ────────────────────────────────────────────────────────────────

pub struct AppState {
    host_db: Arc<HostDb>,
    vault: Arc<parking_lot::Mutex<Vault>>,
    data_dir: PathBuf,

    tabs: Vec<TerminalTabState>,
    active_tab: usize,
    selected_host_id: Option<String>,
    hosts: Vec<Host>,
    groups: Vec<(String, Vec<Host>)>,
    vault_unlocked: bool,
    show_vault_dialog: bool,
    show_host_editor: bool,
    show_key_gen: bool,
    show_sftp: bool,
    status_message: String,

    host_form: HostForm,
    keygen_form: KeyGenForm,
    vault_password: String,
    vault_error: Option<String>,
}

#[derive(Clone)]
struct TerminalTabState {
    id: String,
    host_label: String,
    connected: bool,
}

#[derive(Clone, Default)]
struct HostForm {
    label: String,
    hostname: String,
    port: String,
    username: String,
    auth_type: String,
    group: String,
}

#[derive(Clone, Default)]
struct KeyGenForm {
    label: String,
    key_type: String,
    passphrase: String,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let host_db = Arc::new(HostDb::open(&data_dir).expect("Failed to open host database"));
        let vault = Arc::new(parking_lot::Mutex::new(
            Vault::open(&data_dir).expect("Failed to open vault"),
        ));

        let hosts = host_db.list_hosts(None).unwrap_or_default();
        let groups = Self::group_hosts(&hosts);
        let vault_unlocked = vault.lock().is_unlocked();

        Self {
            host_db,
            vault,
            data_dir,
            tabs: vec![],
            active_tab: 0,
            selected_host_id: None,
            hosts,
            groups,
            vault_unlocked,
            show_vault_dialog: !vault_unlocked,
            show_host_editor: false,
            show_key_gen: false,
            show_sftp: false,
            status_message: String::new(),
            host_form: HostForm { port: "22".into(), username: "root".into(), auth_type: "key".into(), ..Default::default() },
            keygen_form: KeyGenForm { key_type: "ed25519".into(), ..Default::default() },
            vault_password: String::new(),
            vault_error: None,
        }
    }

    fn group_hosts(hosts: &[Host]) -> Vec<(String, Vec<Host>)> {
        let mut map: std::collections::BTreeMap<String, Vec<Host>> = std::collections::BTreeMap::new();
        let mut ungrouped = vec![];
        for h in hosts {
            if let Some(ref g) = h.group_name {
                map.entry(g.clone()).or_default().push(h.clone());
            } else {
                ungrouped.push(h.clone());
            }
        }
        let mut result = vec![];
        if !ungrouped.is_empty() {
            result.push(("Ungrouped".into(), ungrouped));
        }
        result.extend(map);
        result
    }

    fn save_host(&mut self, cx: &mut Context<Self>) {
        let id = Uuid::new_v4().to_string();
        let port: u16 = self.host_form.port.parse().unwrap_or(22);
        let auth_method = match self.host_form.auth_type.as_str() {
            "password" => AuthMethod::Password { vault_id: String::new() },
            _ => AuthMethod::Agent,
        };
        let host = Host {
            id,
            label: self.host_form.label.clone(),
            hostname: self.host_form.hostname.clone(),
            port,
            username: self.host_form.username.clone(),
            auth_method,
            group_name: if self.host_form.group.is_empty() { None } else { Some(self.host_form.group.clone()) },
            tags: vec![],
            bastion_id: None,
            keep_alive_secs: 30,
            created_at: 0,
            updated_at: 0,
        };
        if let Err(e) = self.host_db.upsert_host(&host) {
            self.status_message = format!("Save failed: {}", e);
        } else {
            self.refresh_hosts();
            self.status_message = format!("Host '{}' saved", host.label);
        }
        self.show_host_editor = false;
        self.host_form = HostForm { port: "22".into(), username: "root".into(), auth_type: "key".into(), ..Default::default() };
        cx.notify();
    }

    fn unlock_vault(&mut self, cx: &mut Context<Self>) {
        let mut vault = self.vault.lock();
        match vault.unlock(&self.vault_password) {
            Ok(()) => {
                self.vault_unlocked = true;
                self.show_vault_dialog = false;
                self.vault_error = None;
                self.vault_password.clear();
                self.status_message = "Vault unlocked".into();
            }
            Err(_) => {
                self.vault_error = Some("Wrong password".into());
            }
        }
        cx.notify();
    }

    fn open_host(&mut self, host_id: &str, cx: &mut Context<Self>) {
        if let Some(pos) = self.tabs.iter().position(|t| t.id == host_id) {
            self.active_tab = pos;
            cx.notify();
            return;
        }
        if let Some(host) = self.hosts.iter().find(|h| h.id == host_id) {
            let tab = TerminalTabState { id: host.id.clone(), host_label: host.label.clone(), connected: false };
            self.tabs.push(tab);
            self.active_tab = self.tabs.len() - 1;
            self.selected_host_id = Some(host_id.to_string());
            self.status_message = format!("Connecting to {}...", host.label);
            cx.notify();
        }
    }

    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
            if self.active_tab >= self.tabs.len() { self.active_tab = self.active_tab.saturating_sub(1); }
            cx.notify();
        }
    }

    fn refresh_hosts(&mut self) {
        if let Ok(hosts) = self.host_db.list_hosts(None) {
            self.groups = Self::group_hosts(&hosts);
            self.hosts = hosts;
        }
    }
}

// ── Render ───────────────────────────────────────────────────────────────

impl Render for AppState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = div().size_full().flex_col().bg(rgb(0x0a0e14));

        if self.show_vault_dialog {
            return view.child(render_vault_dialog(self, cx));
        }

        view.child(
            div().size_full().flex()
                .child(render_sidebar(self, cx))
                .child(
                    div().flex_1().h_full().flex_col()
                        .child(render_tab_bar(self, cx))
                        .child(render_main_area(self, cx))
                        .child(render_status_bar(self)),
                ),
        )
    }
}

// ── Vault dialog ─────────────────────────────────────────────────────────

fn render_vault_dialog(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    div().size_full().flex().items_center().justify_center().bg(rgb(0x0a0e14))
        .child(
            div().w(px(400.)).p_6().bg(rgb(0x141929)).rounded_lg().border_1().border_color(rgb(0x1e2538)).flex_col().gap_4()
                .child(text_bold("Unlock Vault", px(20.), rgb(0xe1e5ee)))
                .child(text_el("Enter your master password to unlock SSH keys and credentials.", px(13.), rgb(0x8d91a5)))
                .when(state.vault_error.is_some(), |d| {
                    d.child(text_el(state.vault_error.clone().unwrap_or_default(), px(12.), rgb(0xef4444)))
                })
                .child(div().flex().gap_2()
                    .child(div().flex_1().child(text_el("New vault", px(11.), rgb(0x4a4f62))))
                    .child(
                        div().id("btn-unlock").px_2().py_1().bg(rgb(0x1e2538)).rounded_md().cursor_pointer()
                            .child(text_el("Unlock", px(12.), rgb(0xe1e5ee)))
                            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| { this.unlock_vault(cx); }))
                    )
                ),
        )
}

// ── Sidebar ──────────────────────────────────────────────────────────────

fn render_sidebar(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    div().w(px(260.)).h_full().bg(rgb(0x0d1117)).border_r_1().border_color(rgb(0x1e2538)).flex_col()
        .child(
            div().h(px(40.)).px_3().flex().items_center().justify_between().border_b_1().border_color(rgb(0x1e2538))
                .child(text_bold("Hosts", px(13.), rgb(0xe1e5ee)))
                .child(div().flex().gap_1()
                    .child(div().id("btn-add-host").px_2().py_1().bg(rgb(0x1e2538)).rounded_md().cursor_pointer()
                        .child(text_el("+ Host", px(12.), rgb(0xe1e5ee)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| { this.show_host_editor = true; cx.notify(); })))
                    .child(div().id("btn-keygen").px_2().py_1().bg(rgb(0x1e2538)).rounded_md().cursor_pointer()
                        .child(text_el("🔑", px(12.), rgb(0xe1e5ee)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| { this.show_key_gen = true; cx.notify(); })))
                ),
        )
        .child(
            div().id("host-list").flex_1().overflow_y_scroll().children(
                state.groups.iter().flat_map(|(group_name, hosts)| {
                    let mut children: Vec<gpui::AnyElement> = vec![
                        div().h(px(28.)).px_3().flex().items_center()
                            .child(text_bold(group_name.clone(), px(11.), rgb(0x5a5f73)))
                            .into_any_element()
                    ];
                    for host in hosts {
                        let is_selected = state.selected_host_id.as_deref() == Some(&host.id);
                        let host_id = host.id.clone();
                        let label = host.label.clone();
                        let hostname = host.hostname.clone();
                        let connected = state.tabs.iter().any(|t| t.id == host.id && t.connected);
                        children.push(
                            div().id(host_id.clone()).h(px(32.)).px_3().pl_6().flex().items_center().gap_2()
                                .when(is_selected, |d| d.bg(rgb(0x1a2744)))
                                .hover(|d| d.bg(rgb(0x141d2e)))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| { this.open_host(&host_id.clone(), cx); }))
                                .child(div().w(px(6.)).h(px(6.)).rounded_full().bg(if connected { rgb(0x22c55e) } else { rgb(0x3b3f54) }))
                                .child(text_el(label.clone(), px(13.), if is_selected { rgb(0xe1e5ee) } else { rgb(0x8d91a5) }))
                                .child(div().flex_1())
                                .child(text_el(hostname.clone(), px(10.), rgb(0x4a4f62)))
                                .into_any_element()
                        );
                    }
                    children
                }).collect::<Vec<_>>()
            )
        )
        .when(state.show_host_editor, |parent| {
            parent.child(
                div().absolute().size_full().flex().items_center().justify_center()
                    .bg(rgba(0x00000099))
                    .child(
                        div().w(px(440.)).p_6().bg(rgb(0x141929)).rounded_lg().border_1().border_color(rgb(0x1e2538)).flex_col().gap_3()
                            .child(text_bold("New Host", px(18.), rgb(0xe1e5ee)))
                            .child(form_field("Label", "Production DB"))
                            .child(form_field("Hostname", "10.0.1.50"))
                            .child(form_field("Username", "root"))
                            .child(form_field("Port", "22"))
                            .child(
                                div().flex().gap_2().justify_end().mt_3()
                                    .child(div().id("btn-cancel").px_2().py_1().bg(rgb(0x1e2538)).rounded_md().cursor_pointer()
                                        .child(text_el("Cancel", px(12.), rgb(0xe1e5ee)))
                                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| { this.show_host_editor = false; cx.notify(); })))
                                    .child(div().id("btn-save").px_2().py_1().bg(rgb(0x1e2538)).rounded_md().cursor_pointer()
                                        .child(text_el("Save", px(12.), rgb(0xe1e5ee)))
                                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| { this.save_host(cx); })))
                            )
                    )
            )
        })
}

fn form_field(_label: &str, _placeholder: &str) -> impl IntoElement {
    div().flex_col().gap_1()
        .child(text_el(_label, px(11.), rgb(0x8d91a5)))
        .child(div().h(px(32.)).px_2().bg(rgb(0x0d1117)).rounded_md().border_1().border_color(rgb(0x1e2538)).flex().items_center()
            .child(text_el(_placeholder, px(13.), rgb(0x4a4f62))))
}

// ── Tab bar ──────────────────────────────────────────────────────────────

fn render_tab_bar(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    div().id("tab-bar").h(px(36.)).bg(rgb(0x0d1117)).border_b_1().border_color(rgb(0x1e2538)).flex().overflow_x_scroll()
        .children(state.tabs.iter().enumerate().map(|(i, tab)| {
            let is_active = i == state.active_tab;
            let tab_idx = i;
            div().id(ElementId::Integer(i as u64)).h_full().px_3().flex().items_center().gap_2().border_r_1().border_color(rgb(0x1e2538))
                .when(is_active, |d| d.bg(rgb(0x0a0e14)).border_b_2().border_color(rgb(0x3b82f6)))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| { this.active_tab = tab_idx; cx.notify(); }))
                .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(if tab.connected { rgb(0x22c55e) } else { rgb(0xeab308) }))
                .child(text_el(tab.host_label.clone(), px(12.), rgb(0xe1e5ee)))
                .child(div().id(ElementId::Name(format!("close-tab-{i}").into())).w(px(20.)).h(px(20.)).flex().items_center().justify_center().rounded_md()
                    .hover(|d| d.bg(rgb(0x1e2538))).cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| { this.close_tab(tab_idx, cx); }))
                    .child(text_el("×", px(14.), rgb(0x6b7280))))
        }))
        .child(div().flex_1())
        .child(div().id("btn-sftp").h_full().px_3().flex().items_center().cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| { this.show_sftp = !this.show_sftp; cx.notify(); }))
            .child(text_el("SFTP", px(11.), rgb(0x8d91a5))))
}

// ── Main area ────────────────────────────────────────────────────────────

fn render_main_area(state: &AppState, _cx: &mut Context<AppState>) -> impl IntoElement {
    if state.tabs.is_empty() {
        div().flex_1().size_full().flex().items_center().justify_center().flex_col().gap_3().bg(rgb(0x0a0e14))
            .child(text_bold("ShellMounter", px(24.), rgb(0xe1e5ee)))
            .child(text_el("Add a host (+) or generate a key (🔑) to get started", px(14.), rgb(0x6b7280)))
            .child(text_el(format!("Data: {}", dirs::data_dir().unwrap_or_default().join("shellmounter").display()), px(11.), rgb(0x4a4f62)))
    } else if state.show_sftp {
        div().flex_1().size_full().flex_col().bg(rgb(0x0a0e14))
            .child(div().flex_1().flex().items_center().justify_center().child(text_el("SFTP Browser", px(14.), rgb(0x8d91a5))))
    } else {
        let tab = &state.tabs[state.active_tab];
        div().flex_1().size_full().flex().items_center().justify_center().bg(rgb(0x0a0e14))
            .child(text_el(
                if tab.connected { format!("Connected to {}", tab.host_label) } else { format!("Connecting to {}...", tab.host_label) },
                px(14.),
                if tab.connected { rgb(0x22c55e) } else { rgb(0xeab308) },
            ))
    }
}

// ── Status bar ───────────────────────────────────────────────────────────

fn render_status_bar(state: &AppState) -> impl IntoElement {
    div().h(px(24.)).bg(rgb(0x0d1117)).border_t_1().border_color(rgb(0x1e2538)).px_3().flex().items_center()
        .child(text_el(state.status_message.clone(), px(11.), rgb(0x4a4f62)))
        .child(div().flex_1())
        .child(text_el(format!("{} hosts", state.hosts.len()), px(11.), rgb(0x4a4f62)))
        .child(text_el(" · ", px(11.), rgb(0x2a2f42)))
        .child(text_el(if state.vault_unlocked { "vault open" } else { "vault locked" }, px(11.), if state.vault_unlocked { rgb(0x22c55e) } else { rgb(0xef4444) }))
        .child(text_el(" · ", px(11.), rgb(0x2a2f42)))
        .child(text_el(format!("v{}", env!("CARGO_PKG_VERSION")), px(11.), rgb(0x4a4f62)))
}

// ── Entry point ──────────────────────────────────────────────────────────

pub fn run(data_dir: PathBuf) {
    application().run(move |cx: &mut App| {
        let data_dir = data_dir.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(100.), px(100.)),
                    size(px(1200.), px(800.)),
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("ShellMounter".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_window, cx| cx.new(move |_cx| AppState::new(data_dir.clone())),
        ).unwrap();
    });
}
