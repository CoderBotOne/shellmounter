use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    h_flex,
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu,
        SidebarMenuItem, SidebarToggleButton,
    },
    v_flex, ActiveTheme, Icon, IconName, Root, Sizable, TitleBar,
};
use gpui_platform::application;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::hosts::{AuthMethod, Host, HostDb};
use crate::ssh::keys::{self, KeyType, SshKey};
use crate::ssh::snippets::{Snippet, SnippetStore};
use crate::vault::store::Vault;

// ── Colores de avatar ─────────────────────────────────────────────────────

const AVATAR_COLORS: [u32; 6] = [0xef4444, 0x6366f1, 0x22c55e, 0xa855f7, 0xf97316, 0x0ea5e9];

fn avatar_color(id: &str) -> u32 {
    let idx = id.bytes().fold(0usize, |a, b| a.wrapping_add(b as usize)) % AVATAR_COLORS.len();
    AVATAR_COLORS[idx]
}

// ── Sección de navegación ─────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Default)]
enum Nav {
    #[default]
    Hosts,
    Keychain,
    PortForwarding,
    Snippets,
    KnownHosts,
    Logs,
}

// ── Estado de la app ──────────────────────────────────────────────────────

pub struct AppState {
    host_db: Arc<HostDb>,
    vault: Arc<parking_lot::Mutex<Vault>>,
    snippet_store: Option<SnippetStore>,
    data_dir: PathBuf,

    nav: Nav,
    sidebar_collapsed: bool,
    tabs: Vec<TabState>,
    active_tab: usize,
    selected_host_id: Option<String>,
    hosts: Vec<Host>,
    groups: Vec<(String, Vec<Host>)>,
    vault_unlocked: bool,
    show_host_editor: bool,
    show_key_gen: bool,
    status_message: String,
    host_form: HostForm,
    key_gen_form: KeyGenForm,
    available_keys: Vec<SshKey>,
}

#[derive(Clone)]
struct TabState {
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
    auth_type: String, // "key", "password", "agent"
    selected_key_id: Option<String>,
    group: String,
}

#[derive(Clone)]
struct KeyGenForm {
    label: String,
    key_type: String, // "ed25519", "ecdsa-p256"
    passphrase: String,
}

impl Default for KeyGenForm {
    fn default() -> Self {
        Self { label: String::new(), key_type: "ed25519".into(), passphrase: String::new() }
    }
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let host_db = Arc::new(HostDb::open(&data_dir).expect("open db"));
        let vault = Arc::new(parking_lot::Mutex::new(
            Vault::open(&data_dir).expect("open vault"),
        ));
        let snippet_store = SnippetStore::open(&data_dir.join("snippets.db")).ok();
        let hosts = host_db.list_hosts(None).unwrap_or_default();
        let groups = Self::group_hosts(&hosts);
        let vault_unlocked = vault.lock().is_unlocked();

        Self {
            host_db,
            vault,
            snippet_store,
            data_dir,
            nav: Nav::Hosts,
            sidebar_collapsed: false,
            tabs: vec![],
            active_tab: 0,
            selected_host_id: None,
            hosts,
            groups,
            vault_unlocked,
            show_host_editor: false,
            show_key_gen: false,
            status_message: String::new(),
            host_form: HostForm {
                port: "22".into(),
                username: "root".into(),
                auth_type: "key".into(),
                ..Default::default()
            },
            key_gen_form: KeyGenForm::default(),
            available_keys: vec![],
        }
    }

    fn group_hosts(hosts: &[Host]) -> Vec<(String, Vec<Host>)> {
        let mut map: std::collections::BTreeMap<String, Vec<Host>> = Default::default();
        let mut ungrouped = vec![];
        for h in hosts {
            if let Some(ref g) = h.group_name {
                map.entry(g.clone()).or_default().push(h.clone());
            } else {
                ungrouped.push(h.clone());
            }
        }
        let mut r = vec![];
        if !ungrouped.is_empty() {
            r.push(("Hosts".into(), ungrouped));
        }
        r.extend(map);
        r
    }

    fn load_keys(&mut self) {
        let vault = self.vault.lock();
        if vault.is_unlocked() {
            // Keys are stored in the vault as SshKey blobs
            if let Ok(ids) = vault.list_ids() {
                let mut keys = vec![];
                for id in &ids {
                    if let Ok(data) = vault.get(id) {
                        if let Ok(key) = serde_json::from_slice::<SshKey>(&data) {
                            keys.push(key);
                        }
                    }
                }
                self.available_keys = keys;
            }
        }
    }

    fn save_host(&mut self, cx: &mut Context<Self>) {
        let id = Uuid::new_v4().to_string();
        let port: u16 = self.host_form.port.parse().unwrap_or(22);
        let auth_method = match self.host_form.auth_type.as_str() {
            "password" => AuthMethod::Password {
                vault_id: String::new(),
            },
            "key" => {
                let vault_id = self.host_form.selected_key_id.clone().unwrap_or_default();
                AuthMethod::Key { vault_id }
            }
            _ => AuthMethod::Agent,
        };
        let host = Host {
            id,
            label: self.host_form.label.clone(),
            hostname: self.host_form.hostname.clone(),
            port,
            username: self.host_form.username.clone(),
            auth_method,
            group_name: if self.host_form.group.is_empty() {
                None
            } else {
                Some(self.host_form.group.clone())
            },
            tags: vec![],
            bastion_id: None,
            keep_alive_secs: 30,
            created_at: 0,
            updated_at: 0,
        };
        if let Err(e) = self.host_db.upsert_host(&host) {
            self.status_message = format!("Error: {e}");
        } else {
            self.refresh_hosts();
            self.status_message = format!("Host '{}' guardado", host.label);
        }
        self.show_host_editor = false;
        self.host_form = HostForm {
            port: "22".into(),
            username: "root".into(),
            auth_type: "key".into(),
            ..Default::default()
        };
        cx.notify();
    }

    fn generate_key(&mut self, cx: &mut Context<Self>) {
        let key_type = KeyType::from_str(&self.key_gen_form.label); // actually from key_type field
        let kt = match self.key_gen_form.key_type.as_str() {
            "ecdsa-p256" => KeyType::EcdsaP256,
            _ => KeyType::Ed25519,
        };
        let label = if self.key_gen_form.label.is_empty() {
            format!("key-{}", chrono_now().len())
        } else {
            self.key_gen_form.label.clone()
        };

        match keys::generate(&label, kt, &self.key_gen_form.passphrase) {
            Ok(key) => {
                // Store in vault
                let vault = self.vault.lock();
                if vault.is_unlocked() {
                    let key_id = Uuid::new_v4().to_string();
                    if let Ok(json) = serde_json::to_vec(&key) {
                        let _ = vault.put(
                            &key_id,
                            &key.label,
                            crate::vault::store::SecretKind::SshKey,
                            &json,
                        );
                    }
                }
                self.available_keys.push(key);
                self.status_message = format!("Key '{}' generada", label);
                self.show_key_gen = false;
                self.key_gen_form = KeyGenForm::default();
            }
            Err(e) => {
                self.status_message = format!("Error generando key: {e}");
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
            self.tabs.push(TabState {
                id: host.id.clone(),
                host_label: host.label.clone(),
                connected: false,
            });
            self.active_tab = self.tabs.len() - 1;
            self.selected_host_id = Some(host_id.to_string());
            self.status_message = format!("Conectando a {}...", host.label);
            cx.notify();
        }
    }

    fn close_tab(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.tabs.len() {
            self.tabs.remove(idx);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.active_tab.saturating_sub(1);
            }
            cx.notify();
        }
    }

    fn refresh_hosts(&mut self) {
        if let Ok(hosts) = self.host_db.list_hosts(None) {
            self.groups = Self::group_hosts(&hosts);
            self.hosts = hosts;
        }
    }

    fn save_snippet(&mut self, snippet: &Snippet, cx: &mut Context<Self>) {
        if let Some(ref store) = self.snippet_store {
            match store.save(snippet) {
                Ok(()) => self.status_message = "Snippet guardado".into(),
                Err(e) => self.status_message = format!("Error: {e}"),
            }
            cx.notify();
        }
    }
}

fn chrono_now() -> Vec<u8> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_le_bytes()
        .to_vec()
}

// ── Render principal ──────────────────────────────────────────────────────

impl Render for AppState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let nav = self.nav;
        let collapsed = self.sidebar_collapsed;
        let vault_unlocked = self.vault_unlocked;
        let icon_collapsed = collapsed && true;

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(
                TitleBar::new()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                SidebarToggleButton::new()
                                    .collapsed(icon_collapsed)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sidebar_collapsed = !this.sidebar_collapsed;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_sm()
                                    .child("ShellMounter"),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} hosts · v{}",
                                self.hosts.len(),
                                env!("CARGO_PKG_VERSION")
                            )),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        Sidebar::new("main-sidebar")
                            .w(px(240.))
                            .collapsible(SidebarCollapsible::Icon)
                            .collapsed(collapsed)
                            .header(
                                SidebarHeader::new()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .size_8()
                                            .flex_shrink_0()
                                            .rounded(cx.theme().radius)
                                            .bg(cx.theme().sidebar_primary)
                                            .text_color(cx.theme().sidebar_primary_foreground)
                                            .child(Icon::new(IconName::SquareTerminal)),
                                    )
                                    .when(!icon_collapsed, |this| {
                                        this.child(
                                            v_flex()
                                                .flex_1()
                                                .overflow_hidden()
                                                .child(
                                                    div()
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_sm()
                                                        .child("ShellMounter"),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child("SSH Client"),
                                                ),
                                        )
                                    }),
                            )
                            .child(
                                SidebarGroup::new("Navigation").child(
                                    SidebarMenu::new()
                                        .child(
                                            SidebarMenuItem::new("Hosts")
                                                .icon(IconName::Network)
                                                .active(nav == Nav::Hosts)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.nav = Nav::Hosts;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            SidebarMenuItem::new("Keychain")
                                                .icon(IconName::HardDrive)
                                                .active(nav == Nav::Keychain)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.nav = Nav::Keychain;
                                                    this.load_keys();
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            SidebarMenuItem::new("Port Forwarding")
                                                .icon(IconName::Network)
                                                .active(nav == Nav::PortForwarding)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.nav = Nav::PortForwarding;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            SidebarMenuItem::new("Snippets")
                                                .icon(IconName::SquareTerminal)
                                                .active(nav == Nav::Snippets)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.nav = Nav::Snippets;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            SidebarMenuItem::new("Known Hosts")
                                                .icon(IconName::Globe)
                                                .active(nav == Nav::KnownHosts)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.nav = Nav::KnownHosts;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            SidebarMenuItem::new("Logs")
                                                .icon(IconName::Inbox)
                                                .active(nav == Nav::Logs)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.nav = Nav::Logs;
                                                    cx.notify();
                                                })),
                                        ),
                                ),
                            )
                            .footer(
                                SidebarFooter::new().child(
                                    h_flex()
                                        .gap_2()
                                        .child(
                                            div()
                                                .size_2()
                                                .rounded_full()
                                                .flex_shrink_0()
                                                .bg(if vault_unlocked {
                                                    rgb(0x22c55e)
                                                } else {
                                                    rgb(0xef4444)
                                                }),
                                        )
                                        .when(!icon_collapsed, |this| {
                                            this.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(if vault_unlocked {
                                                        "Vault abierto"
                                                    } else {
                                                        "Vault bloqueado"
                                                    }),
                                            )
                                        }),
                                ),
                            ),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .child(render_content(self, cx))
                            .child(render_status_bar(self, cx)),
                    ),
            )
            .when(self.show_host_editor, |d| {
                d.child(render_host_editor(self, cx))
            })
            .when(self.show_key_gen, |d| {
                d.child(render_key_gen_modal(self, cx))
            })
    }
}

// ── Contenido principal ───────────────────────────────────────────────────

fn render_content(state: &AppState, cx: &mut Context<AppState>) -> AnyElement {
    match state.nav {
        Nav::Hosts if !state.tabs.is_empty() => render_terminal_area(state, cx).into_any_element(),
        Nav::Hosts => render_hosts_view(state, cx).into_any_element(),
        Nav::Keychain => render_keychain_view(state, cx).into_any_element(),
        Nav::Snippets => render_snippets_view(state, cx).into_any_element(),
        _ => render_placeholder(state).into_any_element(),
    }
}

// ── Vista de hosts ────────────────────────────────────────────────────────

fn render_hosts_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex()
        .flex_1()
        .size_full()
        .child(
            h_flex()
                .h_12()
                .px_4()
                .gap_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .id("btn-add-host")
                        .h_8()
                        .px_3()
                        .rounded(cx.theme().radius)
                        .flex()
                        .items_center()
                        .gap_1()
                        .bg(cx.theme().primary)
                        .text_color(cx.theme().primary_foreground)
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .child("+ Nuevo host")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_host_editor = true;
                            cx.notify();
                        })),
                )
                .child(div().flex_1())
                .child(
                    h_flex()
                        .h_8()
                        .px_3()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().secondary)
                        .items_center()
                        .gap_1()
                        .child(Icon::new(IconName::Search).small())
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("Buscar hosts..."),
                        ),
                ),
        )
        .child(
            div()
                .id("host-scroll")
                .flex_1()
                .overflow_y_scroll()
                .p_4()
                .children({
                    let mut items: Vec<AnyElement> = vec![];
                    if state.hosts.is_empty() {
                        items.push(render_empty_state(cx).into_any_element());
                    } else {
                        for (group_name, hosts) in &state.groups {
                            items.push(
                                v_flex()
                                    .mb_4()
                                    .gap_1()
                                    .child(
                                        div()
                                            .px_1()
                                            .mb_1()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(cx.theme().muted_foreground)
                                            .child(group_name.clone()),
                                    )
                                    .children(hosts.iter().map(|h| render_host_card(h, state, cx)))
                                    .into_any_element(),
                            );
                        }
                    }
                    items
                }),
        )
}

fn render_host_card(host: &Host, state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let host_id = host.id.clone();
    let label = host.label.clone();
    let hostname = host.hostname.clone();
    let username = host.username.clone();
    let port = host.port;
    let is_selected = state.selected_host_id.as_deref() == Some(&host.id);
    let connected = state.tabs.iter().any(|t| t.id == host.id && t.connected);
    let acolor = avatar_color(&host.id);
    let first: SharedString = label
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into())
        .into();

    h_flex()
        .id(host_id.clone())
        .w_full()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .gap_3()
        .bg(cx.theme().background)
        .border_1()
        .border_color(if is_selected {
            cx.theme().primary
        } else {
            cx.theme().border
        })
        .cursor_pointer()
        .hover(|d| d.bg(cx.theme().accent))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_host(&host_id, cx);
        }))
        .child(
            div()
                .size_9()
                .rounded(cx.theme().radius)
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .bg(rgb(acolor))
                .text_color(rgb(0xffffff))
                .font_weight(FontWeight::BOLD)
                .text_sm()
                .child(first),
        )
        .child(
            v_flex()
                .flex_1()
                .overflow_hidden()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().foreground)
                        .child(label),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("ssh  {}@{}:{}", username, hostname, port)),
                ),
        )
        .when(connected, |d| {
            d.child(
                div()
                    .size_2()
                    .rounded_full()
                    .flex_shrink_0()
                    .bg(rgb(0x22c55e)),
            )
        })
}

fn render_empty_state(cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .pt_16()
        .child(Icon::new(IconName::Network).large())
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_base()
                .child("Sin hosts todavía"),
        )
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Haz clic en \"+ Nuevo host\" para agregar tu primer servidor SSH."),
        )
}

fn render_placeholder(state: &AppState) -> impl IntoElement {
    let label = match state.nav {
        Nav::Keychain => "Keychain",
        Nav::PortForwarding => "Port Forwarding",
        Nav::Snippets => "Snippets",
        Nav::KnownHosts => "Known Hosts",
        Nav::Logs => "Logs",
        Nav::Hosts => "Hosts",
    };
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            v_flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_xl()
                        .child(label),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(gpui::rgb(0x9aa3bf))
                        .child("Próximamente"),
                ),
        )
}

// ── Keychain ──────────────────────────────────────────────────────────────

fn render_keychain_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let unlocked = state.vault_unlocked;

    v_flex()
        .flex_1()
        .size_full()
        .child(
            h_flex()
                .h_12()
                .px_4()
                .gap_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .id("btn-gen-key")
                        .h_8()
                        .px_3()
                        .rounded(cx.theme().radius)
                        .flex()
                        .items_center()
                        .gap_1()
                        .bg(cx.theme().primary)
                        .text_color(cx.theme().primary_foreground)
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .child("+ Nueva Key")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.vault_unlocked {
                                this.show_key_gen = true;
                            } else {
                                this.status_message = "Desbloquea el vault primero".into();
                            }
                            cx.notify();
                        })),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{} keys", state.available_keys.len())),
                ),
        )
        .child(
            div()
                .flex_1()
                .overflow_y_scroll()
                .p_4()
                .children(if !unlocked {
                    vec![v_flex()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .pt_16()
                        .gap_2()
                        .child(Icon::new(IconName::HardDrive).large())
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_base()
                                .child("Vault bloqueado"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("Desbloquea el vault para ver tus keys SSH."),
                        )
                        .into_any_element()]
                } else if state.available_keys.is_empty() {
                    vec![v_flex()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .pt_16()
                        .gap_2()
                        .child(Icon::new(IconName::HardDrive).large())
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_base()
                                .child("Sin keys"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("Genera una nueva key SSH para conectar a tus servidores."),
                        )
                        .into_any_element()]
                } else {
                    state.available_keys.iter().map(|key| {
                        render_key_card(key, cx)
                    }).collect()
                }),
        )
}

fn render_key_card(key: &SshKey, cx: &mut Context<AppState>) -> AnyElement {
    let acolor = avatar_color(&key.fingerprint);
    let first: SharedString = key.label
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "K".into())
        .into();

    h_flex()
        .w_full()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .gap_3()
        .bg(cx.theme().background)
        .border_1()
        .border_color(cx.theme().border)
        .mb_1()
        .child(
            div()
                .size_9()
                .rounded(cx.theme().radius)
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .bg(rgb(acolor))
                .text_color(rgb(0xffffff))
                .font_weight(FontWeight::BOLD)
                .text_xs()
                .child(first),
        )
        .child(
            v_flex()
                .flex_1()
                .overflow_hidden()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().foreground)
                        .child(key.label.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{} — {}", key.key_type.display_name(), &key.fingerprint[..16])),
                ),
        )
        .into_any_element()
}

// ── Modal generar key ─────────────────────────────────────────────────────

fn render_key_gen_modal(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgba(0x00000066))
        .child(
            v_flex()
                .w(px(420.))
                .rounded_xl()
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .p_6()
                .gap_4()
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_base()
                                .child("Nueva Key SSH"),
                        )
                        .child(
                            div()
                                .id("btn-close-keygen")
                                .size_6()
                                .rounded(cx.theme().radius)
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(cx.theme().secondary)
                                .cursor_pointer()
                                .hover(|d| d.bg(cx.theme().secondary_hover))
                                .child(Icon::new(IconName::Close).small())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_key_gen = false;
                                    cx.notify();
                                })),
                        ),
                )
                .child(form_field("Label", "ej: servidor-prod", cx))
                .child(
                    h_flex()
                        .gap_2()
                        .child({
                            let is_ed = &state.key_gen_form.key_type == "ed25519";
                            div()
                                .flex_1()
                                .h_9()
                                .rounded(cx.theme().radius)
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .bg(if is_ed { cx.theme().primary } else { cx.theme().secondary })
                                .text_color(if is_ed { cx.theme().primary_foreground } else { cx.theme().foreground })
                                .child("Ed25519")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.key_gen_form.key_type = "ed25519".into();
                                    cx.notify();
                                }))
                        })
                        .child({
                            let is_ec = &state.key_gen_form.key_type == "ecdsa-p256";
                            div()
                                .flex_1()
                                .h_9()
                                .rounded(cx.theme().radius)
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .bg(if is_ec { cx.theme().primary } else { cx.theme().secondary })
                                .text_color(if is_ec { cx.theme().primary_foreground } else { cx.theme().foreground })
                                .child("ECDSA P-256")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.key_gen_form.key_type = "ecdsa-p256".into();
                                    cx.notify();
                                }))
                        }),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .justify_end()
                        .pt_1()
                        .child(
                            div()
                                .px_4()
                                .py_2()
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().secondary)
                                .border_1()
                                .border_color(cx.theme().border)
                                .text_sm()
                                .cursor_pointer()
                                .hover(|d| d.bg(cx.theme().secondary_hover))
                                .child("Cancelar")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_key_gen = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("btn-gen")
                                .px_4()
                                .py_2()
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().primary)
                                .text_color(cx.theme().primary_foreground)
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .hover(|d| d.bg(cx.theme().primary_hover))
                                .child("Generar")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.generate_key(cx);
                                })),
                        ),
                ),
        )
}

// ── Snippets ──────────────────────────────────────────────────────────────

fn render_snippets_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let snippets = state.snippet_store.as_ref()
        .and_then(|s| s.list().ok())
        .unwrap_or_default();

    v_flex()
        .flex_1()
        .size_full()
        .child(
            h_flex()
                .h_12()
                .px_4()
                .gap_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .h_8()
                        .px_3()
                        .rounded(cx.theme().radius)
                        .flex()
                        .items_center()
                        .gap_1()
                        .bg(cx.theme().primary)
                        .text_color(cx.theme().primary_foreground)
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .child("+ Nuevo snippet"),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{} snippets", snippets.len())),
                ),
        )
        .child(
            div()
                .flex_1()
                .overflow_y_scroll()
                .p_4()
                .children(if snippets.is_empty() {
                    vec![v_flex()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .pt_16()
                        .gap_2()
                        .child(Icon::new(IconName::SquareTerminal).large())
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_base()
                                .child("Sin snippets"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("Guarda comandos frecuentes para ejecutar rápido en tus servidores."),
                        )
                        .into_any_element()]
                } else {
                    snippets.iter().map(|s| {
                        h_flex()
                            .w_full()
                            .px_3()
                            .py_2()
                            .rounded(cx.theme().radius)
                            .gap_3()
                            .bg(cx.theme().background)
                            .border_1()
                            .border_color(cx.theme().border)
                            .mb_1()
                            .child(
                                Icon::new(IconName::SquareTerminal).small()
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .overflow_hidden()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(cx.theme().foreground)
                                            .child(s.label.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .font_family("monospace".into())
                                            .child(s.command.clone()),
                                    ),
                            )
                            .into_any_element()
                    }).collect()
                }),
        )
}

// ── Terminal ──────────────────────────────────────────────────────────────

fn render_terminal_area(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex()
        .flex_1()
        .size_full()
        .bg(rgb(0x0d1117))
        .child(render_tab_bar(state, cx))
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child({
                    let tab = &state.tabs[state.active_tab];
                    div()
                        .text_sm()
                        .text_color(if tab.connected {
                            rgb(0x22c55e)
                        } else {
                            rgb(0xeab308)
                        })
                        .child(if tab.connected {
                            format!("Conectado a {}", tab.host_label)
                        } else {
                            format!("Conectando a {}...", tab.host_label)
                        })
                }),
        )
}

fn render_tab_bar(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    div()
        .id("tab-bar")
        .h_9()
        .bg(rgb(0x141929))
        .border_b_1()
        .border_color(rgb(0x1e2538))
        .flex()
        .overflow_x_scroll()
        .children(state.tabs.iter().enumerate().map(|(i, tab)| {
            let is_active = i == state.active_tab;
            let ti = i;
            h_flex()
                .id(ElementId::Integer(i as u64))
                .h_full()
                .px_4()
                .gap_2()
                .border_r_1()
                .border_color(rgb(0x1e2538))
                .when(is_active, |d| {
                    d.bg(rgb(0x0d1117)).border_b_2().border_color(rgb(0x5b7cf6))
                })
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.active_tab = ti;
                    cx.notify();
                }))
                .child(
                    div()
                        .size_2()
                        .rounded_full()
                        .bg(rgb(if tab.connected { 0x22c55e } else { 0xeab308 })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(if is_active { 0xdde3f8 } else { 0x7b84a8 }))
                        .child(tab.host_label.clone()),
                )
                .child(
                    div()
                        .id(ElementId::Name(format!("x-{i}").into()))
                        .size_4()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .text_xs()
                        .text_color(rgb(0x7b84a8))
                        .hover(|d| d.bg(rgb(0x232942)).text_color(rgb(0xdde3f8)))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_tab(ti, cx);
                        }))
                        .child("\u{00D7}"),
                )
        }))
}

// ── Modal nuevo host ──────────────────────────────────────────────────────

fn render_host_editor(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgba(0x00000066))
        .child(
            v_flex()
                .w(px(460.))
                .rounded_xl()
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .p_6()
                .gap_4()
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_base()
                                .child("Nuevo Host"),
                        )
                        .child(
                            div()
                                .id("btn-close-modal")
                                .size_6()
                                .rounded(cx.theme().radius)
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(cx.theme().secondary)
                                .cursor_pointer()
                                .hover(|d| d.bg(cx.theme().secondary_hover))
                                .child(Icon::new(IconName::Close).small())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_host_editor = false;
                                    cx.notify();
                                })),
                        ),
                )
                .child(form_field("Label", "Producción DB", cx))
                .child(form_field("Hostname / IP", "10.0.1.50", cx))
                .child(
                    h_flex()
                        .gap_3()
                        .child(div().flex_1().child(form_field("Usuario", "root", cx)))
                        .child(div().w(px(80.)).child(form_field("Puerto", "22", cx))),
                )
                // Auth type selector
                .child(
                    v_flex()
                        .gap_1p5()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().foreground)
                                .child("Autenticación"),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child({
                                    let selected = &state.host_form.auth_type == "key";
                                    div()
                                        .flex_1()
                                        .h_9()
                                        .rounded(cx.theme().radius)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .cursor_pointer()
                                        .bg(if selected { cx.theme().primary } else { cx.theme().secondary })
                                        .text_color(if selected { cx.theme().primary_foreground } else { cx.theme().foreground })
                                        .child("SSH Key")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.host_form.auth_type = "key".into();
                                            cx.notify();
                                        }))
                                })
                                .child({
                                    let selected = &state.host_form.auth_type == "password";
                                    div()
                                        .flex_1()
                                        .h_9()
                                        .rounded(cx.theme().radius)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .cursor_pointer()
                                        .bg(if selected { cx.theme().primary } else { cx.theme().secondary })
                                        .text_color(if selected { cx.theme().primary_foreground } else { cx.theme().foreground })
                                        .child("Password")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.host_form.auth_type = "password".into();
                                            cx.notify();
                                        }))
                                })
                                .child({
                                    let selected = &state.host_form.auth_type == "agent";
                                    div()
                                        .flex_1()
                                        .h_9()
                                        .rounded(cx.theme().radius)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .cursor_pointer()
                                        .bg(if selected { cx.theme().primary } else { cx.theme().secondary })
                                        .text_color(if selected { cx.theme().primary_foreground } else { cx.theme().foreground })
                                        .child("Agent")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.host_form.auth_type = "agent".into();
                                            cx.notify();
                                        }))
                                }),
                        ),
                )
                // Key selector (only shown when auth_type == "key")
                .when(&state.host_form.auth_type == "key", |d| {
                    d.child(
                        v_flex()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(cx.theme().foreground)
                                    .child("SSH Key"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .h_9()
                                    .px_3()
                                    .rounded(cx.theme().radius)
                                    .bg(cx.theme().background)
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .flex()
                                    .items_center()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if state.available_keys.is_empty() {
                                        "No hay keys — genera una en Keychain".to_string()
                                    } else if let Some(ref kid) = state.host_form.selected_key_id {
                                        state.available_keys.iter()
                                            .find(|k| &k.fingerprint == kid)
                                            .map(|k| k.label.clone())
                                            .unwrap_or_else(|| "Seleccionar key...".into())
                                    } else {
                                        "Seleccionar key...".to_string()
                                    }),
                            )
                            .when(!state.available_keys.is_empty(), |d| {
                                d.child(
                                    v_flex()
                                        .gap_0p5()
                                        .max_h(px(160.))
                                        .overflow_y_scroll()
                                        .border_1()
                                        .border_color(cx.theme().border)
                                        .rounded(cx.theme().radius)
                                        .children(state.available_keys.iter().map(|k| {
                                            let fp = k.fingerprint.clone();
                                            let label = k.label.clone();
                                            let is_sel = state.host_form.selected_key_id.as_deref() == Some(&fp);
                                            h_flex()
                                                .px_3()
                                                .py_2()
                                                .gap_2()
                                                .cursor_pointer()
                                                .bg(if is_sel { cx.theme().accent } else { cx.theme().background })
                                                .hover(|d| d.bg(cx.theme().accent))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.host_form.selected_key_id = Some(fp.clone());
                                                    cx.notify();
                                                }))
                                                .child(Icon::new(IconName::HardDrive).small())
                                                .child(
                                                    v_flex()
                                                        .gap_0p5()
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .text_color(cx.theme().foreground)
                                                                .child(label.clone()),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(cx.theme().muted_foreground)
                                                                .child(format!("{}", &fp[..20])),
                                                        ),
                                                )
                                                .into_any_element()
                                        }))
                                )
                            }),
                    )
                })
                .child(
                    h_flex()
                        .gap_2()
                        .justify_end()
                        .pt_1()
                        .child(
                            div()
                                .id("btn-cancel")
                                .px_4()
                                .py_2()
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().secondary)
                                .border_1()
                                .border_color(cx.theme().border)
                                .text_sm()
                                .cursor_pointer()
                                .hover(|d| d.bg(cx.theme().secondary_hover))
                                .child("Cancelar")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_host_editor = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("btn-save")
                                .px_4()
                                .py_2()
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().primary)
                                .text_color(cx.theme().primary_foreground)
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .hover(|d| d.bg(cx.theme().primary_hover))
                                .child("Guardar")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.save_host(cx);
                                })),
                        ),
                ),
        )
}

fn form_field(label: &str, placeholder: &str, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex()
        .gap_1p5()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .child(label.to_string()),
        )
        .child(
            div()
                .h_9()
                .px_3()
                .rounded(cx.theme().radius)
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .flex()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(placeholder.to_string()),
                ),
        )
}

// ── Barra de estado ───────────────────────────────────────────────────────

fn render_status_bar(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    h_flex()
        .h_6()
        .px_4()
        .gap_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().title_bar)
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(state.status_message.clone()),
        )
        .child(div().flex_1())
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!(
                    "{} hosts · v{}",
                    state.hosts.len(),
                    env!("CARGO_PKG_VERSION")
                )),
        )
}

// ── Entry point ───────────────────────────────────────────────────────────

pub fn run(data_dir: PathBuf) {
    let app = application().with_assets(gpui_component_assets::Assets);

    app.run(move |cx: &mut App| {
        gpui_component::init(cx);

        let data_dir = data_dir.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(100.), px(100.)),
                    size(px(1200.), px(800.)),
                ))),
                #[cfg(not(target_os = "linux"))]
                titlebar: Some(TitleBar::title_bar_options()),
                #[cfg(target_os = "linux")]
                window_background: WindowBackgroundAppearance::Transparent,
                #[cfg(target_os = "linux")]
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            move |window, cx| {
                let state = cx.new(move |_cx| AppState::new(data_dir.clone()));
                cx.new(|cx| Root::new(state, window, cx))
            },
        )
        .unwrap();
    });
}
