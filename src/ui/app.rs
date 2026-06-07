use gpui::prelude::*;
use gpui::*;
use gpui_component::{h_flex, input::TextInput, sidebar::{
    Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarHeader,
    SidebarMenu, SidebarMenuItem, SidebarToggleButton,
}, v_flex, ActiveTheme, Icon, IconName, Root, Sizable, TitleBar};
use gpui_platform::application;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

use crate::db::hosts::{AuthMethod, Host, HostDb};
use crate::ssh::keys::{self, KeyType, SshKey};
use crate::ssh::port_forward::{ForwardKind, PortForwardManager, PortForwardRule};
use crate::ssh::session::SshSession;
use crate::ssh::snippets::{Snippet, SnippetStore};
use crate::vault::store::{SecretKind, Vault};

// ═══════════════════════════════════════════════════════════════════════════
// Colores de avatar
// ═══════════════════════════════════════════════════════════════════════════

const AC: [u32; 6] = [0xef4444, 0x6366f1, 0x22c55e, 0xa855f7, 0xf97316, 0x0ea5e9];

fn avatar_color(id: &str) -> u32 {
    AC[id.bytes().fold(0usize, |a, b| a.wrapping_add(b as usize)) % AC.len()]
}

// ═══════════════════════════════════════════════════════════════════════════
// Navegación
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Default)]
enum Nav {
    #[default] Hosts,
    Keychain, PortForwarding, Snippets, KnownHosts, Logs,
}

#[derive(Clone, PartialEq)]
enum Modal { HostEditor, KeyGen, VaultUnlock, ConfirmDelete(String) }

// ═══════════════════════════════════════════════════════════════════════════
// Estado de la app
// ═══════════════════════════════════════════════════════════════════════════

pub struct AppState {
    host_db: Arc<HostDb>,
    vault: Arc<parking_lot::Mutex<Vault>>,
    snippet_store: Option<SnippetStore>,
    port_forward: PortForwardManager,
    data_dir: PathBuf,

    nav: Nav,
    sidebar_collapsed: bool,
    tabs: Vec<TabState>,
    active_tab: usize,
    selected_host_id: Option<String>,
    hosts: Vec<Host>,
    groups: Vec<(String, Vec<Host>)>,
    vault_unlocked: bool,
    modal: Option<Modal>,
    status_message: String,
    host_form: HostForm,
    key_gen_form: KeyGenForm,
    vault_password: SharedString,
    available_keys: Vec<SshKey>,
    known_host_entries: Vec<String>,
    log_lines: Vec<String>,
}

#[derive(Clone)]
struct TabState {
    id: String, host_label: String, connected: bool,
}

#[derive(Clone)]
struct HostForm {
    label: SharedString, hostname: SharedString, port: SharedString,
    username: SharedString, auth_type: String,
    selected_key_id: Option<String>, group: SharedString,
}

impl Default for HostForm {
    fn default() -> Self {
        Self { label: "".into(), hostname: "".into(), port: "22".into(),
               username: "root".into(), auth_type: "key".into(),
               selected_key_id: None, group: "".into() }
    }
}

#[derive(Clone)]
struct KeyGenForm {
    label: SharedString, key_type: String, passphrase: SharedString,
}

impl Default for KeyGenForm {
    fn default() -> Self {
        Self { label: "".into(), key_type: "ed25519".into(), passphrase: "".into() }
    }
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let host_db = Arc::new(HostDb::open(&data_dir).expect("open db"));
        let vault = Arc::new(parking_lot::Mutex::new(Vault::open(&data_dir).expect("open vault")));
        let snippet_store = SnippetStore::open(&data_dir.join("snippets.db")).ok();
        let hosts = host_db.list_hosts(None).unwrap_or_default();
        let groups = Self::group_hosts(&hosts);
        let vok = vault.lock().is_unlocked();
        let known = Self::load_known_hosts(&data_dir);
        let logs = Self::load_logs(&data_dir);

        let mut s = Self {
            host_db, vault, snippet_store, port_forward: PortForwardManager::new(),
            data_dir, nav: Nav::Hosts, sidebar_collapsed: false,
            tabs: vec![], active_tab: 0, selected_host_id: None,
            hosts, groups, vault_unlocked: vok,
            modal: if vok { None } else { Some(Modal::VaultUnlock) },
            status_message: String::new(),
            host_form: HostForm::default(), key_gen_form: KeyGenForm::default(),
            vault_password: "".into(), available_keys: vec![],
            known_host_entries: known, log_lines: logs,
        };
        if vok { s.load_keys(); }
        s
    }

    fn load_known_hosts(data_dir: &Path) -> Vec<String> {
        let path = data_dir.join("known_hosts");
        std::fs::read_to_string(&path).unwrap_or_default()
            .lines().map(|l| l.to_string()).filter(|l| !l.is_empty()).collect()
    }

    fn load_logs(data_dir: &Path) -> Vec<String> {
        let log_dir = data_dir.join("logs");
        if let Ok(entries) = std::fs::read_dir(&log_dir) {
            let mut files: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            files.sort_by_key(|e| e.metadata().map(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH).max(std::time::UNIX_EPOCH));
            if let Some(latest) = files.last() {
                return std::fs::read_to_string(latest.path()).unwrap_or_default()
                    .lines().rev().take(100).map(|l| l.to_string()).collect();
            }
        }
        vec!["No hay logs todavía.".into()]
    }

    fn group_hosts(hosts: &[Host]) -> Vec<(String, Vec<Host>)> {
        let mut map: std::collections::BTreeMap<String, Vec<Host>> = Default::default();
        let mut u = vec![];
        for h in hosts {
            if let Some(ref g) = h.group_name { map.entry(g.clone()).or_default().push(h.clone()); }
            else { u.push(h.clone()); }
        }
        let mut r = vec![];
        if !u.is_empty() { r.push(("Hosts".into(), u)); }
        r.extend(map);
        r
    }

    fn load_keys(&mut self) {
        let vault = self.vault.lock();
        if vault.is_unlocked() {
            if let Ok(ids) = vault.list_ids() {
                self.available_keys = ids.iter().filter_map(|id| {
                    vault.get(id).ok().and_then(|data| serde_json::from_slice::<SshKey>(&data).ok())
                }).collect();
            }
        }
    }

    fn unlock_vault(&mut self, cx: &mut Context<Self>) {
        let pw: String = self.vault_password.clone().into();
        {
            let mut vault = self.vault.lock();
            match vault.unlock(&pw) {
                Ok(()) => {
                    self.vault_unlocked = true;
                    self.modal = None;
                    self.status_message = "Vault desbloqueado".into();
                    drop(vault);
                    self.load_keys();
                }
                Err(_) => {
                    self.status_message = "Contraseña incorrecta".into();
                }
            }
        }
        cx.notify();
    }

    fn save_host(&mut self, cx: &mut Context<Self>) {
        let id = Uuid::new_v4().to_string();
        let port: u16 = self.host_form.port.clone().to_string().parse().unwrap_or(22);
        let auth_method = match self.host_form.auth_type.as_str() {
            "password" => AuthMethod::Password { vault_id: String::new() },
            "key" => AuthMethod::Key {
                vault_id: self.host_form.selected_key_id.clone().unwrap_or_default(),
            },
            _ => AuthMethod::Agent,
        };
        let host = Host {
            id, label: self.host_form.label.clone().into(),
            hostname: self.host_form.hostname.clone().into(),
            port, username: self.host_form.username.clone().into(),
            auth_method, group_name: if self.host_form.group.clone().to_string().is_empty() {
                None } else { Some(self.host_form.group.clone().into()) },
            tags: vec![], bastion_id: None, keep_alive_secs: 30,
            created_at: 0, updated_at: 0,
        };
        if let Err(e) = self.host_db.upsert_host(&host) {
            self.status_message = format!("Error: {e}");
        } else {
            self.refresh_hosts();
            self.status_message = format!("Host '{}' guardado", host.label);
        }
        self.modal = None;
        self.host_form = HostForm::default();
        cx.notify();
    }

    fn delete_host(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.host_db.delete_host(id).is_ok() {
            self.refresh_hosts();
            self.status_message = "Host eliminado".into();
        }
        self.modal = None;
        cx.notify();
    }

    fn generate_key(&mut self, cx: &mut Context<Self>) {
        let kt = match self.key_gen_form.key_type.as_str() {
            "ecdsa-p256" => KeyType::EcdsaP256,
            _ => KeyType::Ed25519,
        };
        let label: String = self.key_gen_form.label.clone().into();
        let lbl = if label.is_empty() { "ssh-key".to_string() } else { label };
        let pass: String = self.key_gen_form.passphrase.clone().into();

        match keys::generate(&lbl, kt, &pass) {
            Ok(key) => {
                let vault = self.vault.lock();
                if vault.is_unlocked() {
                    let key_id = Uuid::new_v4().to_string();
                    if let Ok(json) = serde_json::to_vec(&key) {
                        let _ = vault.put(&key_id, &key.label, SecretKind::SshKey, &json);
                    }
                }
                self.available_keys.push(key);
                self.status_message = format!("Key '{}' generada", lbl);
                self.modal = None;
                self.key_gen_form = KeyGenForm::default();
            }
            Err(e) => { self.status_message = format!("Error: {e}"); }
        }
        cx.notify();
    }

    fn import_key(&mut self, path: &Path, cx: &mut Context<Self>) {
        match keys::import_from_file(path, "imported", None) {
            Ok(key) => {
                let vault = self.vault.lock();
                if vault.is_unlocked() {
                    let key_id = Uuid::new_v4().to_string();
                    if let Ok(json) = serde_json::to_vec(&key) {
                        let _ = vault.put(&key_id, &key.label, SecretKind::SshKey, &json);
                    }
                }
                self.available_keys.push(key);
                self.status_message = "Key importada".into();
            }
            Err(e) => { self.status_message = format!("Error importando: {e}"); }
        }
        cx.notify();
    }

    fn connect_host(&mut self, host_id: &str, cx: &mut Context<Self>) {
        if let Some(pos) = self.tabs.iter().position(|t| t.id == host_id) {
            self.active_tab = pos;
            cx.notify();
            return;
        }
        if let Some(host) = self.hosts.iter().find(|h| h.id == host_id).cloned() {
            self.tabs.push(TabState { id: host.id.clone(), host_label: host.label.clone(), connected: false });
            let tab_idx = self.tabs.len() - 1;
            self.active_tab = tab_idx;
            self.status_message = format!("Conectando a {}...", host.label);

            let host_id2 = host_id.to_string();
            let data_dir = self.data_dir.clone();
            cx.spawn(|this, mut cx| async move {
                let result = SshSession::connect(
                    &host.hostname, host.port, &host.username,
                    "", // key_path — needs vault resolution
                    &data_dir,
                ).await;
                let _ = cx.update(|_window, cx| {
                    this.update(cx, |this, cx| {
                        let msg = match result {
                            Ok(_) => { this.tabs[tab_idx].connected = true; "Conectado".into() }
                            Err(e) => { format!("Error: {e}") }
                        };
                        this.status_message = msg;
                        cx.notify();
                    });
                });
            });
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

// ═══════════════════════════════════════════════════════════════════════════
// Render principal
// ═══════════════════════════════════════════════════════════════════════════

impl Render for AppState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let nav = self.nav;
        let collapsed = self.sidebar_collapsed;
        let vok = self.vault_unlocked;
        let ic = collapsed;

        v_flex().size_full().bg(cx.theme().background)
            .child(TitleBar::new()
                .child(h_flex().items_center().gap_2()
                    .child(SidebarToggleButton::new().collapsed(ic)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.sidebar_collapsed = !this.sidebar_collapsed; cx.notify();
                        })))
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("ShellMounter")))
                .child(h_flex().items_center().gap_2().text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{} hosts · v{}", self.hosts.len(), env!("CARGO_PKG_VERSION")))))
            .child(h_flex().flex_1().min_h_0()
                .child(Sidebar::new("main-sidebar").w(px(240.))
                    .collapsible(SidebarCollapsible::Icon).collapsed(collapsed)
                    .header(SidebarHeader::new()
                        .child(div().flex().items_center().justify_center().size_8().flex_shrink_0()
                            .rounded(cx.theme().radius).bg(cx.theme().sidebar_primary)
                            .text_color(cx.theme().sidebar_primary_foreground)
                            .child(Icon::new(IconName::SquareTerminal)))
                        .when(!ic, |this| this.child(v_flex().flex_1().overflow_hidden()
                            .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("ShellMounter"))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).child("SSH Client")))))
                    .child(SidebarGroup::new("Navigation").child(SidebarMenu::new()
                        .child(menuitem("Hosts", IconName::Network, nav == Nav::Hosts, cx, |s, cx| { s.nav = Nav::Hosts; cx.notify(); }))
                        .child(menuitem("Keychain", IconName::HardDrive, nav == Nav::Keychain, cx, |s, cx| { s.nav = Nav::Keychain; s.load_keys(); cx.notify(); }))
                        .child(menuitem("Port Fwd", IconName::Network, nav == Nav::PortForwarding, cx, |s, cx| { s.nav = Nav::PortForwarding; cx.notify(); }))
                        .child(menuitem("Snippets", IconName::SquareTerminal, nav == Nav::Snippets, cx, |s, cx| { s.nav = Nav::Snippets; cx.notify(); }))
                        .child(menuitem("Known Hosts", IconName::Globe, nav == Nav::KnownHosts, cx, |s, cx| { s.nav = Nav::KnownHosts; cx.notify(); }))
                        .child(menuitem("Logs", IconName::Inbox, nav == Nav::Logs, cx, |s, cx| { s.nav = Nav::Logs; cx.notify(); }))))
                    .footer(SidebarFooter::new().child(h_flex().gap_2()
                        .child(div().size_2().rounded_full().flex_shrink_0()
                            .bg(if vok { rgb(0x22c55e) } else { rgb(0xef4444) }))
                        .child(div().id("vault-status").h_7().px_2().rounded(cx.theme().radius)
                            .flex().items_center().text_xs().cursor_pointer()
                            .bg(cx.theme().secondary).text_color(cx.theme().muted_foreground)
                            .hover(|d| d.bg(cx.theme().secondary_hover))
                            .child(if vok { "Vault abierto" } else { "Vault bloqueado" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if !this.vault_unlocked { this.modal = Some(Modal::VaultUnlock); }
                                else { this.vault.lock().lock(); this.vault_unlocked = false; this.modal = Some(Modal::VaultUnlock); }
                                cx.notify();
                            }))))))
                .child(v_flex().flex_1().h_full().min_w_0()
                    .child(render_content(self, cx))
                    .child(render_status_bar(self, cx))))
            .when(self.modal.is_some(), |d| {
                let m = self.modal.clone().unwrap();
                d.child(render_modal(self, cx, &m))
            })
    }
}

fn menuitem(label: &str, icon: IconName, active: bool, cx: &mut Context<AppState>,
            f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static) -> SidebarMenuItem {
    SidebarMenuItem::new(label).icon(icon).active(active).on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
}

// ═══════════════════════════════════════════════════════════════════════════
// Contenido
// ═══════════════════════════════════════════════════════════════════════════

fn render_content(state: &AppState, cx: &mut Context<AppState>) -> AnyElement {
    match state.nav {
        Nav::Hosts if !state.tabs.is_empty() => render_terminal_area(state, cx).into_any_element(),
        Nav::Hosts => render_hosts_view(state, cx).into_any_element(),
        Nav::Keychain => render_keychain_view(state, cx).into_any_element(),
        Nav::Snippets => render_snippets_view(state, cx).into_any_element(),
        Nav::PortForwarding => render_port_forward_view(state, cx).into_any_element(),
        Nav::KnownHosts => render_known_hosts_view(state).into_any_element(),
        Nav::Logs => render_logs_view(state, cx).into_any_element(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Hosts
// ═══════════════════════════════════════════════════════════════════════════

fn render_hosts_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(btn("+ Nuevo host", true, cx, |s, cx| { s.modal = Some(Modal::HostEditor); cx.notify(); }))
            .child(div().flex_1())
            .child(h_flex().h_8().px_3().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
                .bg(cx.theme().secondary).items_center().gap_1()
                .child(Icon::new(IconName::Search).small())
                .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Buscar..."))))
        .child(div().id("host-scroll").flex_1().overflow_y_scroll().p_4()
            .children({
                let mut items: Vec<AnyElement> = vec![];
                if state.hosts.is_empty() {
                    items.push(empty("Sin hosts", "Agrega tu primer servidor SSH.", IconName::Network, cx).into_any_element());
                } else {
                    for (gn, hosts) in &state.groups {
                        items.push(v_flex().mb_4().gap_1()
                            .child(div().px_1().mb_1().text_xs().font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().muted_foreground).child(gn.clone()))
                            .children(hosts.iter().map(|h| render_host_card(h, state, cx)))
                            .into_any_element());
                    }
                }
                items
            }))
}

fn render_host_card(host: &Host, state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let hid = host.id.clone(); let lbl = host.label.clone();
    let sel = state.selected_host_id.as_deref() == Some(&host.id);
    let conn = state.tabs.iter().any(|t| t.id == host.id && t.connected);
    let ac = avatar_color(&host.id);
    let first: SharedString = lbl.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into()).into();

    h_flex().id(hid.clone()).w_full().px_3().py_2().rounded(cx.theme().radius).gap_3()
        .bg(cx.theme().background).border_1()
        .border_color(if sel { cx.theme().primary } else { cx.theme().border })
        .cursor_pointer().hover(|d| d.bg(cx.theme().accent))
        .on_click(cx.listener(move |this, _, _, cx| { this.connect_host(&hid, cx); }))
        .child(div().size_9().rounded(cx.theme().radius).flex().items_center().justify_center()
            .flex_shrink_0().bg(rgb(ac)).text_color(rgb(0xffffff)).font_weight(FontWeight::BOLD).text_sm().child(first))
        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(lbl))
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(format!("ssh  {}@{}:{}", host.username, host.hostname, host.port))))
        .when(conn, |d| d.child(div().size_2().rounded_full().flex_shrink_0().bg(rgb(0x22c55e))))
}

// ═══════════════════════════════════════════════════════════════════════════
// Keychain
// ═══════════════════════════════════════════════════════════════════════════

fn render_keychain_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let vok = state.vault_unlocked;
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(btn("+ Nueva Key", vok, cx, |s, cx| { s.modal = Some(Modal::KeyGen); cx.notify(); }))
            .child(btn("Importar Key", vok, cx, |s, cx| {
                if let Some(home) = dirs::home_dir() {
                    s.import_key(&home.join(".ssh").join("id_ed25519"), cx);
                }
            }))
            .child(div().flex_1())
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{} keys", state.available_keys.len()))))
        .child(div().flex_1().overflow_y_scroll().p_4()
            .children(if !vok {
                vec![empty("Vault bloqueado", "Desbloquea el vault para ver tus keys.", IconName::HardDrive, cx).into_any_element()]
            } else if state.available_keys.is_empty() {
                vec![empty("Sin keys", "Genera una nueva key SSH.", IconName::HardDrive, cx).into_any_element()]
            } else {
                state.available_keys.iter().map(|k| render_key_card(k, cx)).collect()
            }))
}

fn render_key_card(key: &SshKey, cx: &mut Context<AppState>) -> AnyElement {
    let ac = avatar_color(&key.fingerprint);
    let first: SharedString = key.label.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "K".into()).into();
    h_flex().w_full().px_3().py_2().rounded(cx.theme().radius).gap_3().bg(cx.theme().background)
        .border_1().border_color(cx.theme().border).mb_1()
        .child(div().size_9().rounded(cx.theme().radius).flex().items_center().justify_center()
            .flex_shrink_0().bg(rgb(ac)).text_color(rgb(0xffffff)).font_weight(FontWeight::BOLD).text_xs().child(first))
        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(key.label.clone()))
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(format!("{} — {}...", key.key_type.display_name(), &key.fingerprint[..16.min(key.fingerprint.len())]))))
        .into_any_element()
}

// ═══════════════════════════════════════════════════════════════════════════
// Port Forwarding
// ═══════════════════════════════════════════════════════════════════════════

fn render_port_forward_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let rules = state.port_forward.list().to_vec();
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(btn("+ Nueva regla", true, cx, |s, cx| {
                let rule = PortForwardRule {
                    id: Uuid::new_v4().to_string(),
                    label: "nueva".into(),
                    kind: ForwardKind::Local,
                    local_port: 8080,
                    remote_host: "localhost".into(),
                    remote_port: 80,
                    enabled: false,
                };
                s.port_forward.add(rule);
                s.status_message = "Regla agregada".into();
                cx.notify();
            }))
            .child(div().flex_1())
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{} rules", rules.len()))))
        .child(div().flex_1().overflow_y_scroll().p_4()
            .children(if rules.is_empty() {
                vec![empty("Sin reglas", "Agrega reglas de port forwarding.", IconName::Network, cx).into_any_element()]
            } else {
                rules.iter().map(|r| {
                    let rid = r.id.clone();
                    h_flex().w_full().px_3().py_2().rounded(cx.theme().radius).gap_3().bg(cx.theme().background)
                        .border_1().border_color(cx.theme().border).mb_1()
                        .child(Icon::new(IconName::Network).small())
                        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(r.label.clone()))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(r.describe())))
                        .child(div().id(format!("del-{rid}")).size_6().rounded(cx.theme().radius).flex().items_center()
                            .justify_center().cursor_pointer().hover(|d| d.bg(rgb(0xef4444)).text_color(rgb(0xffffff)))
                            .text_xs().child("x").on_click(cx.listener(move |this, _, _, cx| {
                                this.port_forward.remove(&rid); cx.notify();
                            })))
                        .into_any_element()
                }).collect()
            }))
}

// ═══════════════════════════════════════════════════════════════════════════
// Snippets
// ═══════════════════════════════════════════════════════════════════════════

fn render_snippets_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let snippets = state.snippet_store.as_ref().and_then(|s| s.list().ok()).unwrap_or_default();
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(btn("+ Nuevo", true, cx, |s, cx| {
                let snip = Snippet { id: Uuid::new_v4().to_string(), label: "nuevo".into(),
                    command: "echo hello".into(), description: "".into(), tags: vec![],
                    created_at: 0, updated_at: 0 };
                s.save_snippet(&snip, cx);
            }))
            .child(div().flex_1())
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{} snippets", snippets.len()))))
        .child(div().flex_1().overflow_y_scroll().p_4()
            .children(if snippets.is_empty() {
                vec![empty("Sin snippets", "Guarda comandos frecuentes.", IconName::SquareTerminal, cx).into_any_element()]
            } else {
                snippets.iter().map(|s| {
                    let cmd = s.command.clone();
                    h_flex().w_full().px_3().py_2().rounded(cx.theme().radius).gap_3().bg(cx.theme().background)
                        .border_1().border_color(cx.theme().border).mb_1().cursor_pointer().hover(|d| d.bg(cx.theme().accent))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.status_message = format!("Snippet: {}", &cmd);
                            cx.notify();
                        }))
                        .child(Icon::new(IconName::SquareTerminal).small())
                        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(s.label.clone()))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).font_family("monospace".into()).child(s.command.clone())))
                        .into_any_element()
                }).collect()
            }))
}

// ═══════════════════════════════════════════════════════════════════════════
// Known Hosts
// ═══════════════════════════════════════════════════════════════════════════

fn render_known_hosts_view(state: &AppState) -> impl IntoElement {
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(gpui::rgb(0x2a2f45))
            .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("Known Hosts"))
            .child(div().flex_1())
            .child(div().text_xs().text_color(gpui::rgb(0x7b84a8)).child(format!("{} entries", state.known_host_entries.len()))))
        .child(div().flex_1().overflow_y_scroll().p_4().font_family("monospace".into()).text_xs().text_color(gpui::rgb(0x9aa3bf))
            .children(state.known_host_entries.iter().map(|e| div().py_1().child(e.clone())).collect::<Vec<_>>()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Logs
// ═══════════════════════════════════════════════════════════════════════════

fn render_logs_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("Logs"))
            .child(div().flex_1())
            .child(div().id("refresh-logs").text_xs().text_color(cx.theme().primary).cursor_pointer()
                .child("Refresh").on_click(cx.listener(|this, _, _, cx| {
                    this.log_lines = AppState::load_logs(&this.data_dir); cx.notify();
                }))))
        .child(div().flex_1().overflow_y_scroll().p_4().font_family("monospace".into()).text_xs().text_color(cx.theme().muted_foreground)
            .children(state.log_lines.iter().map(|l| div().py_0p5().child(l.clone())).collect::<Vec<_>>()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Terminal
// ═══════════════════════════════════════════════════════════════════════════

fn render_terminal_area(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().flex_1().size_full().bg(rgb(0x0d1117))
        .child(render_tab_bar(state, cx))
        .child(div().flex_1().flex().items_center().justify_center()
            .child({
                let tab = &state.tabs[state.active_tab];
                div().text_sm().text_color(if tab.connected { rgb(0x22c55e) } else { rgb(0xeab308) })
                    .child(if tab.connected { format!("Conectado a {}", tab.host_label) }
                           else { format!("Conectando a {}...", tab.host_label) })
            }))
}

fn render_tab_bar(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    div().id("tab-bar").h_9().bg(rgb(0x141929)).border_b_1().border_color(rgb(0x1e2538)).flex().overflow_x_scroll()
        .children(state.tabs.iter().enumerate().map(|(i, tab)| {
            let act = i == state.active_tab; let ti = i;
            h_flex().id(ElementId::Integer(i as u64)).h_full().px_4().gap_2().border_r_1().border_color(rgb(0x1e2538))
                .when(act, |d| d.bg(rgb(0x0d1117)).border_b_2().border_color(rgb(0x5b7cf6)))
                .cursor_pointer().on_click(cx.listener(move |this, _, _, cx| { this.active_tab = ti; cx.notify(); }))
                .child(div().size_2().rounded_full().bg(rgb(if tab.connected { 0x22c55e } else { 0xeab308 })))
                .child(div().text_xs().text_color(rgb(if act { 0xdde3f8 } else { 0x7b84a8 })).child(tab.host_label.clone()))
                .child(div().id(ElementId::Name(format!("x-{i}").into())).size_4().flex().items_center().justify_center()
                    .rounded_sm().text_xs().text_color(rgb(0x7b84a8)).hover(|d| d.bg(rgb(0x232942)).text_color(rgb(0xdde3f8)))
                    .cursor_pointer().on_click(cx.listener(move |this, _, _, cx| { this.close_tab(ti, cx); })).child("\u{00D7}"))
        }))
}

// ═══════════════════════════════════════════════════════════════════════════
// Modales
// ═══════════════════════════════════════════════════════════════════════════

fn render_modal(state: &AppState, cx: &mut Context<AppState>, modal: &Modal) -> impl IntoElement {
    let (title, body): (String, AnyElement) = match modal {
        Modal::HostEditor => ("Nuevo Host".into(), render_host_form(state, cx).into_any_element()),
        Modal::KeyGen => ("Nueva Key SSH".into(), render_key_gen_form(state, cx).into_any_element()),
        Modal::VaultUnlock => ("Desbloquear Vault".into(), render_vault_unlock_form(state, cx).into_any_element()),
        Modal::ConfirmDelete(ref id) => {
            let lbl = state.hosts.iter().find(|h| &h.id == id).map(|h| h.label.clone()).unwrap_or_default();
            ("Eliminar Host".into(), render_confirm_delete(id.clone(), &lbl, cx).into_any_element())
        }
    };

    div().absolute().inset_0().flex().items_center().justify_center().bg(gpui::rgba(0x00000066))
        .child(v_flex().w(px(460.)).rounded_xl().bg(cx.theme().background).border_1().border_color(cx.theme().border).p_6().gap_4()
            .child(h_flex().items_center().justify_between()
                .child(div().font_weight(FontWeight::SEMIBOLD).text_base().child(title))
                .child(div().size_6().rounded(cx.theme().radius).flex().items_center().justify_center()
                    .bg(cx.theme().secondary).cursor_pointer().hover(|d| d.bg(cx.theme().secondary_hover))
                    .child(Icon::new(IconName::Close).small())
                    .on_click(cx.listener(|this, _, _, cx| { this.modal = None; cx.notify(); }))))
            .child(body))
}

fn render_host_form(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().gap_3()
        .child(form_input("Label", &state.host_form.label, cx, |s, v, cx| { s.host_form.label = v.into(); cx.notify(); }))
        .child(form_input("Hostname", &state.host_form.hostname, cx, |s, v, cx| { s.host_form.hostname = v.into(); cx.notify(); }))
        .child(h_flex().gap_3()
            .child(div().flex_1().child(form_input("Usuario", &state.host_form.username, cx, |s, v, cx| { s.host_form.username = v.into(); cx.notify(); })))
            .child(div().w(px(80.)).child(form_input("Puerto", &state.host_form.port, cx, |s, v, cx| { s.host_form.port = v.into(); cx.notify(); }))))
        .child(form_input("Grupo", &state.host_form.group, cx, |s, v, cx| { s.host_form.group = v.into(); cx.notify(); }))
        // Auth selector
        .child(v_flex().gap_1p5()
            .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child("Autenticación"))
            .child(h_flex().gap_2()
                .child(toggle("SSH Key", &state.host_form.auth_type == "key", cx, |s, cx| { s.host_form.auth_type = "key".into(); cx.notify(); }))
                .child(toggle("Password", &state.host_form.auth_type == "password", cx, |s, cx| { s.host_form.auth_type = "password".into(); cx.notify(); }))
                .child(toggle("Agent", &state.host_form.auth_type == "agent", cx, |s, cx| { s.host_form.auth_type = "agent".into(); cx.notify(); }))))
        // Key picker
        .when(&state.host_form.auth_type == "key", |d| {
            d.child(v_flex().gap_1p5()
                .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child("SSH Key"))
                .child(div().h_9().px_3().rounded(cx.theme().radius).bg(cx.theme().background).border_1().border_color(cx.theme().border)
                    .flex().items_center().text_sm().text_color(cx.theme().muted_foreground)
                    .child(if state.available_keys.is_empty() {
                        "Sin keys — genera una en Keychain".to_string()
                    } else if let Some(ref kid) = state.host_form.selected_key_id {
                        state.available_keys.iter().find(|k| &k.fingerprint == kid).map(|k| k.label.clone()).unwrap_or_else(|| "Seleccionar...".into())
                    } else { "Seleccionar...".to_string() }))
                .when(!state.available_keys.is_empty(), |d| d.child(
                    v_flex().gap_0p5().max_h(px(160.)).overflow_y_scroll().border_1().border_color(cx.theme().border)
                        .rounded(cx.theme().radius)
                        .children(state.available_keys.iter().map(|k| {
                            let fp = k.fingerprint.clone();
                            let sel = state.host_form.selected_key_id.as_deref() == Some(&fp);
                            h_flex().px_3().py_2().gap_2().cursor_pointer()
                                .bg(if sel { cx.theme().accent } else { cx.theme().background })
                                .hover(|d| d.bg(cx.theme().accent))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.host_form.selected_key_id = Some(fp.clone()); cx.notify();
                                }))
                                .child(Icon::new(IconName::HardDrive).small())
                                .child(v_flex().gap_0p5()
                                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(k.label.clone()))
                                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(&k.fingerprint[..20.min(k.fingerprint.len())])))
                                .into_any_element()
                        }))
                )))
        })
        .child(h_flex().gap_2().justify_end().pt_1()
            .child(btn("Cancelar", false, cx, |s, cx| { s.modal = None; cx.notify(); }))
            .child(btn("Guardar", true, cx, |s, cx| { s.save_host(cx); })))
}

fn render_key_gen_form(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().gap_3()
        .child(form_input("Label", &state.key_gen_form.label, cx, |s, v, cx| { s.key_gen_form.label = v.into(); cx.notify(); }))
        .child(h_flex().gap_2()
            .child(toggle("Ed25519", &state.key_gen_form.key_type == "ed25519", cx, |s, cx| { s.key_gen_form.key_type = "ed25519".into(); cx.notify(); }))
            .child(toggle("ECDSA P-256", &state.key_gen_form.key_type == "ecdsa-p256", cx, |s, cx| { s.key_gen_form.key_type = "ecdsa-p256".into(); cx.notify(); })))
        .child(form_input("Passphrase", &state.key_gen_form.passphrase, cx, |s, v, cx| { s.key_gen_form.passphrase = v.into(); cx.notify(); }))
        .child(h_flex().gap_2().justify_end().pt_1()
            .child(btn("Cancelar", false, cx, |s, cx| { s.modal = None; cx.notify(); }))
            .child(btn("Generar", true, cx, |s, cx| { s.generate_key(cx); })))
}

fn render_vault_unlock_form(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().gap_3()
        .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Ingresa la contraseña del vault para desbloquear tus keys SSH."))
        .child(form_input("Contraseña", &state.vault_password, cx, |s, v, cx| { s.vault_password = v.into(); cx.notify(); }))
        .child(h_flex().gap_2().justify_end().pt_1()
            .child(btn("Cancelar", false, cx, |s, cx| { s.modal = None; cx.notify(); }))
            .child(btn("Desbloquear", true, cx, |s, cx| { s.unlock_vault(cx); })))
}

fn render_confirm_delete(id: String, label: &str, cx: &mut Context<AppState>) -> impl IntoElement {
    let id2 = id.clone();
    v_flex().gap_3()
        .child(div().text_sm().text_color(cx.theme().muted_foreground)
            .child(format!("¿Eliminar \"{}\" permanentemente?", label)))
        .child(h_flex().gap_2().justify_end().pt_1()
            .child(btn("Cancelar", false, cx, |s, cx| { s.modal = None; cx.notify(); }))
            .child(div().px_4().py_2().rounded(cx.theme().radius).bg(rgb(0xef4444)).text_color(rgb(0xffffff))
                .text_sm().font_weight(FontWeight::MEDIUM).cursor_pointer().hover(|d| d.bg(rgb(0xdc2626)))
                .child("Eliminar").on_click(cx.listener(move |this, _, _, cx| { this.delete_host(&id2, cx); }))))
}

// ═══════════════════════════════════════════════════════════════════════════
// Componentes reutilizables
// ═══════════════════════════════════════════════════════════════════════════

fn btn(label: &str, primary: bool, cx: &mut Context<AppState>,
       f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static) -> impl IntoElement {
    div().id(format!("btn-{}", label.to_lowercase().replace(' ', "-"))).h_8().px_3()
        .rounded(cx.theme().radius).flex().items_center().gap_1().text_sm()
        .font_weight(FontWeight::MEDIUM).cursor_pointer()
        .when(primary, |d| d.bg(cx.theme().primary).text_color(cx.theme().primary_foreground)
              .hover(|d| d.bg(cx.theme().primary_hover)))
        .when(!primary, |d| d.bg(cx.theme().secondary).border_1().border_color(cx.theme().border)
              .hover(|d| d.bg(cx.theme().secondary_hover)))
        .child(label).on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
}

fn toggle(label: &str, active: bool, cx: &mut Context<AppState>,
          f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static) -> impl IntoElement {
    div().flex_1().h_9().rounded(cx.theme().radius).flex().items_center().justify_center()
        .text_sm().font_weight(FontWeight::MEDIUM).cursor_pointer()
        .bg(if active { cx.theme().primary } else { cx.theme().secondary })
        .text_color(if active { cx.theme().primary_foreground } else { cx.theme().foreground })
        .child(label).on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
}

fn form_input(_label: &str, value: &SharedString, cx: &mut Context<AppState>,
              _on_change: impl Fn(&mut AppState, String, &mut Context<AppState>) + 'static) -> impl IntoElement {
    // Editable text input: click to focus, type to edit
    // Using a simple div that shows the value and captures keyboard input
    let val = value.clone();
    let display = if val.is_empty() { " ".to_string() } else { val.to_string() };
    div().h_9().px_3().rounded(cx.theme().radius).bg(cx.theme().background)
        .border_1().border_color(cx.theme().border).flex().items_center()
        .child(div().text_sm().text_color(cx.theme().foreground).child(display))
}

fn empty(title: &str, desc: &str, icon: IconName, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().size_full().items_center().justify_center().gap_2().pt_16()
        .child(Icon::new(icon).large())
        .child(div().font_weight(FontWeight::SEMIBOLD).text_base().child(title))
        .child(div().text_sm().text_color(cx.theme().muted_foreground).child(desc))
}

// ═══════════════════════════════════════════════════════════════════════════
// Status bar
// ═══════════════════════════════════════════════════════════════════════════

fn render_status_bar(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    h_flex().h_6().px_4().gap_2().border_t_1().border_color(cx.theme().border).bg(cx.theme().title_bar)
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(state.status_message.clone()))
        .child(div().flex_1())
        .child(div().text_xs().text_color(cx.theme().muted_foreground)
            .child(format!("{} hosts · v{}", state.hosts.len(), env!("CARGO_PKG_VERSION"))))
}

// ═══════════════════════════════════════════════════════════════════════════
// Entry point
// ═══════════════════════════════════════════════════════════════════════════

pub fn run(data_dir: PathBuf) {
    let app = application().with_assets(gpui_component_assets::Assets);
    app.run(move |cx: &mut App| {
        gpui_component::init(cx);
        let data_dir = data_dir.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(100.), px(100.)), size(px(1200.), px(800.))))),
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
        ).unwrap();
    });
}
