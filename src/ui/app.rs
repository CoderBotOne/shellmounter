use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    h_flex,
    input::{Input, InputState},
    form::{field, v_form},
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu,
        SidebarMenuItem, SidebarToggleButton,
    },
    v_flex, ActiveTheme, Icon, IconName, Root, Sizable,
};
use gpui_component::scroll::ScrollableElement as _;
use gpui_platform::application;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

use crate::db::hosts::{AuthMethod, Host, HostDb};
use crate::ssh::keys::{self, KeyType, SshKey};
use crate::ssh::port_forward::{ForwardKind, PortForwardManager, PortForwardRule};
use crate::ssh::session::{AuthMethod as SshAuth, SshSession};
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
    Keychain, PortForwarding, Snippets, KnownHosts, Logs, Settings, Sftp,
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
    vault_password: Entity<InputState>,
    available_keys: Vec<SshKey>,
    known_host_entries: Vec<String>,
    log_lines: Vec<String>,
    // SFTP file browser state
    sftp: SftpState,
    // Host search filter
    search_query: SharedString,
    /// Focus handle for terminal keyboard input.
    focus_handle: FocusHandle,
    /// Terminal font size in px (default 13, range 8-24).
    terminal_font_size: usize,
}

#[derive(Clone)]
struct SftpState {
    local_path: String,
    local_entries: Vec<crate::fs::FileEntry>,
    local_loading: bool,
    show_hidden: bool,
    selected_host_id: Option<String>,
    // Remote SFTP state
    remote_path: String,
    remote_entries: Vec<crate::fs::FileEntry>,
    remote_loading: bool,
    remote_connected: bool,
    /// Active SFTP session (wrapped for async sharing).
    sftp_session: Option<std::sync::Arc<parking_lot::Mutex<russh_sftp::client::SftpSession>>>,
    /// SSH session handle (needed to keep the connection alive).
    ssh_session: Option<std::sync::Arc<parking_lot::Mutex<SshSession>>>,
}

impl Default for SftpState {
    fn default() -> Self {
        Self {
            local_path: dirs::home_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| "/".into()),
            local_entries: vec![],
            local_loading: false,
            show_hidden: false,
            selected_host_id: None,
            remote_path: "/".into(),
            remote_entries: vec![],
            remote_loading: false,
            remote_connected: false,
            sftp_session: None,
            ssh_session: None,
        }
    }
}

#[derive(Clone)]
struct TabState {
    id: String, host_label: String, connected: bool,
    /// Terminal emulator — wrapped in Arc for sharing with SSH recv task.
    terminal: std::sync::Arc<parking_lot::Mutex<crate::terminal::view::TerminalView>>,
    /// Active SSH session (present when connected).
    session: Option<std::sync::Arc<parking_lot::Mutex<SshSession>>>,
}

impl TabState {
    fn new(id: String, host_label: String) -> Self {
        let term = crate::terminal::view::TerminalView::new(
            crate::terminal::view::TerminalSize::new(120, 40),
        );
        Self {
            id, host_label, connected: false,
            terminal: std::sync::Arc::new(parking_lot::Mutex::new(term)),
            session: None,
        }
    }
}

#[derive(Clone)]
struct HostForm {
    label: Entity<InputState>, hostname: Entity<InputState>, port: Entity<InputState>,
    username: Entity<InputState>, group: Entity<InputState>, password: Entity<InputState>,
    auth_type: String,
    selected_key_id: Option<String>,
    editing_id: Option<String>,
}

impl HostForm {
    fn new(window: &mut Window, cx: &mut Context<AppState>) -> Self {
        Self {
            label: cx.new(|cx| InputState::new(window, cx).placeholder("Label")),
            hostname: cx.new(|cx| InputState::new(window, cx).placeholder("Hostname")),
            username: cx.new(|cx| InputState::new(window, cx).placeholder("Usuario").default_value("root")),
            port: cx.new(|cx| InputState::new(window, cx).placeholder("Puerto").default_value("22")),
            group: cx.new(|cx| InputState::new(window, cx).placeholder("Grupo")),
            password: cx.new(|cx| InputState::new(window, cx).placeholder("Password")),
            auth_type: "key".into(),
            selected_key_id: None,
            editing_id: None,
        }
    }
}

#[derive(Clone)]
struct KeyGenForm {
    label: Entity<InputState>, passphrase: Entity<InputState>,
    key_type: String,
}

impl KeyGenForm {
    fn new(window: &mut Window, cx: &mut Context<AppState>) -> Self {
        Self {
            label: cx.new(|cx| InputState::new(window, cx)),
            passphrase: cx.new(|cx| InputState::new(window, cx)),
            key_type: "ed25519".into(),
        }
    }
}

impl AppState {
    pub fn new(data_dir: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            host_form: HostForm::new(window, cx),
            key_gen_form: KeyGenForm::new(window, cx),
            vault_password: cx.new(|cx| InputState::new(window, cx)),
            available_keys: vec![],
            known_host_entries: known, log_lines: logs,
            sftp: SftpState::default(),
            search_query: "".into(),
            focus_handle: cx.focus_handle(),
            terminal_font_size: 13,
        };
        if vok { s.load_keys(); }
        s
    }

    fn load_local_files(&mut self) {
        self.sftp.local_loading = true;
        if let Ok(entries) = crate::fs::list_local(std::path::Path::new(&self.sftp.local_path), self.sftp.show_hidden) {
            self.sftp.local_entries = entries;
        }
        self.sftp.local_loading = false;
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
            files.sort_by_key(|e| {
                e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            });
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
        let pw = self.vault_password.read(cx).value().to_string();
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
        let editing = self.host_form.editing_id.take();
        let id = editing.unwrap_or_else(|| Uuid::new_v4().to_string());
        let label = self.host_form.label.read(cx).value().to_string();
        let hostname = self.host_form.hostname.read(cx).value().to_string();
        let username = self.host_form.username.read(cx).value().to_string();
        let port: u16 = self.host_form.port.read(cx).value().parse().unwrap_or(22);
        let group = self.host_form.group.read(cx).value().to_string();
        let auth_method = match self.host_form.auth_type.as_str() {
            "password" => {
                let vault_id = Uuid::new_v4().to_string();
                let pw = self.host_form.password.read(cx).value().to_string();
                {
                    let mut vault = self.vault.lock();
                    let _ = vault.put(&vault_id, "", SecretKind::Password, pw.as_bytes());
                }
                AuthMethod::Password { vault_id }
            }
            "key" => AuthMethod::Key {
                vault_id: self.host_form.selected_key_id.clone().unwrap_or_default(),
            },
            _ => AuthMethod::Agent,
        };
        let host = Host {
            id, label, hostname,
            port, username,
            auth_method,
            group_name: if group.is_empty() { None } else { Some(group) },
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
        let label = self.key_gen_form.label.read(cx).value().to_string();
        let lbl = if label.is_empty() { "ssh-key".to_string() } else { label };
        let pass = self.key_gen_form.passphrase.read(cx).value().to_string();

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
        if let Some(host) = self.hosts.iter().find(|h| h.id == host_id).cloned() {
            // Always create a new tab with unique ID — allows multiple sessions to same host
            let unique_tab_id = Uuid::new_v4().to_string();
            let tab = TabState::new(unique_tab_id.clone(), host.label.clone());
            self.tabs.push(tab);
            let tab_idx = self.tabs.len() - 1;
            self.active_tab = tab_idx;
            self.status_message = format!("Conectando a {}...", host.label);

            let host_id2 = host_id.to_string();
            let host_label = host.label.clone();
            let data_dir = self.data_dir.clone();
            let hostname = host.hostname.clone();
            let username = host.username.clone();
            let port = host.port;
            let auth_method = host.auth_method.clone();
            let vault = self.vault.clone();
            let terminal = self.tabs[tab_idx].terminal.clone();
            let tab_id = self.tabs[tab_idx].id.clone();

            // Resolve authentication before spawning
            let auth = match &auth_method {
                AuthMethod::Key { vault_id } => {
                    let vault = vault.lock();
                    match vault.get(vault_id) {
                        Ok(data) => {
                            match serde_json::from_slice::<SshKey>(&data) {
                                Ok(ssh_key) => {
                                    match hex::decode(&ssh_key.private_key_bytes) {
                                        Ok(key_bytes) => Some(SshAuth::Key { key_bytes }),
                                        Err(e) => {
                                            self.status_message = format!("Error decodificando key: {e}");
                                            cx.notify();
                                            return;
                                        }
                                    }
                                }
                                Err(e) => {
                                    self.status_message = format!("Error leyendo key del vault: {e}");
                                    cx.notify();
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            self.status_message = format!("Key no encontrada en vault: {e}");
                            cx.notify();
                            return;
                        }
                    }
                }
                AuthMethod::Password { vault_id } => {
                    let vault = vault.lock();
                    match vault.get(vault_id) {
                        Ok(data) => {
                            match String::from_utf8(data) {
                                Ok(password) => Some(SshAuth::Password { password }),
                                Err(e) => {
                                    self.status_message = format!("Error leyendo password: {e}");
                                    cx.notify();
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            self.status_message = format!("Password no encontrada en vault: {e}");
                            cx.notify();
                            return;
                        }
                    }
                }
                AuthMethod::Agent => Some(SshAuth::Agent),
            };

            let auth = match auth {
                Some(a) => a,
                None => return,
            };

            cx.spawn(async move |entity: gpui::WeakEntity<AppState>, cx| {
                let result = SshSession::connect(
                    &hostname, port, &username, auth, &data_dir,
                ).await;
                match result {
                    Ok(mut session) => {
                        // Request PTY
                        let _ = session.request_pty("xterm-256color", 120, 40).await;
                        let session = std::sync::Arc::new(parking_lot::Mutex::new(session));

                        // Store session in tab
                        entity.update(cx, |this, cx| {
                            if let Some(tab) = this.tabs.iter_mut().find(|t| t.id == tab_id) {
                                tab.connected = true;
                                tab.session = Some(session.clone());
                            }
                            this.status_message = format!("Conectado a {}", host_label);
                            cx.notify();
                        }).ok();

                        // Spawn recv loop
                        let term = terminal.clone();
                        let sess = session.clone();
                        let entity2 = entity.clone();
                        cx.spawn(async move |cx| {
                            loop {
                                let data = {
                                    let mut s = sess.lock();
                                    tokio::time::timeout(
                                        std::time::Duration::from_millis(100),
                                        s.recv(),
                                    ).await
                                };
                                match data {
                                    Ok(Ok(Some(bytes))) => {
                                        let mut t = term.lock();
                                        t.write(&bytes);
                                        drop(t);
                                        // Only notify UI when data arrives
                                        // UI will update on next recv
                                    }
                                    _ => {
                                        // Check if session is still open
                                        let open = sess.lock().is_open();
                                        if !open {
                                            entity2.update(cx, |this, cx| {
                                                if let Some(tab) = this.tabs.iter_mut().find(|t| t.id == tab_id) {
                                                    tab.connected = false;
                                                    tab.session = None;
                                                }
                                                this.status_message = "Desconectado".into();
                                                cx.notify();
                                            }).ok();
                                            break;
                                        }
                                    }
                                }
                            }
                        }).detach();
                    }
                    Err(e) => {
                        entity.update(cx, |this, cx| {
                            if let Some(tab) = this.tabs.iter_mut().find(|t| t.id == tab_id) {
                                tab.connected = false;
                            }
                            this.status_message = format!("Error: {e}");
                            cx.notify();
                        }).ok();
                    }
                }
            }).detach();
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

    /// Connect to a host for SFTP browsing.
    fn connect_sftp_host(&mut self, host_id: &str, cx: &mut Context<Self>) {
        let host = match self.hosts.iter().find(|h| h.id == host_id).cloned() {
            Some(h) => h,
            None => return,
        };

        self.sftp.selected_host_id = Some(host_id.to_string());
        self.sftp.remote_loading = true;
        self.sftp.remote_path = "/".into();
        self.status_message = format!("Conectando SFTP a {}...", host.label);
        cx.notify();

        let host_id2 = host_id.to_string();
        let hostname = host.hostname.clone();
        let username = host.username.clone();
        let port = host.port;
        let auth_method = host.auth_method.clone();
        let vault = self.vault.clone();
        let data_dir = self.data_dir.clone();

        // Resolve auth
        let auth = match &auth_method {
            AuthMethod::Key { vault_id } => {
                let vault = vault.lock();
                match vault.get(vault_id).ok()
                    .and_then(|d| serde_json::from_slice::<SshKey>(&d).ok())
                    .and_then(|k| hex::decode(&k.private_key_bytes).ok())
                {
                    Some(key_bytes) => SshAuth::Key { key_bytes },
                    None => {
                        self.status_message = "Key no encontrada".into();
                        self.sftp.remote_loading = false;
                        cx.notify();
                        return;
                    }
                }
            }
            AuthMethod::Password { vault_id } => {
                let vault = vault.lock();
                match vault.get(vault_id).ok().and_then(|d| String::from_utf8(d).ok()) {
                    Some(password) => SshAuth::Password { password },
                    None => {
                        self.status_message = "Password no encontrada".into();
                        self.sftp.remote_loading = false;
                        cx.notify();
                        return;
                    }
                }
            }
            AuthMethod::Agent => SshAuth::Agent,
        };

        cx.spawn(async move |entity: gpui::WeakEntity<AppState>, cx| {
            // Connect SSH
            let ssh = match SshSession::connect(&hostname, port, &username, auth, &data_dir).await {
                Ok(s) => s,
                Err(e) => {
                    entity.update(cx, |this, cx| {
                        this.status_message = format!("Error SFTP: {e}");
                        this.sftp.remote_loading = false;
                        this.sftp.remote_connected = false;
                        cx.notify();
                    }).ok();
                    return;
                }
            };

            // Open SFTP
            let sftp = match ssh.open_sftp().await {
                Ok(s) => s,
                Err(e) => {
                    entity.update(cx, |this, cx| {
                        this.status_message = format!("Error abriendo SFTP: {e}");
                        this.sftp.remote_loading = false;
                        this.sftp.remote_connected = false;
                        cx.notify();
                    }).ok();
                    return;
                }
            };

            // List root
            match crate::ssh::sftp::list(&sftp, "/").await {
                Ok(entries) => {
                    let file_entries: Vec<crate::fs::FileEntry> = entries.into_iter().map(|e| {
                        crate::fs::FileEntry {
                            name: e.name.clone(),
                            path: format!("/{}", e.name),
                            is_dir: e.is_dir,
                            size: e.size.unwrap_or(0),
                            modified: String::new(),
                        }
                    }).collect();

                    let ssh_arc = std::sync::Arc::new(parking_lot::Mutex::new(ssh));
                    let sftp_arc = std::sync::Arc::new(parking_lot::Mutex::new(sftp));

                    entity.update(cx, |this, cx| {
                        this.sftp.remote_entries = file_entries;
                        this.sftp.remote_loading = false;
                        this.sftp.remote_connected = true;
                        this.sftp.sftp_session = Some(sftp_arc);
                        this.sftp.ssh_session = Some(ssh_arc);
                        this.status_message = format!("SFTP conectado a {}", hostname);
                        cx.notify();
                    }).ok();
                }
                Err(e) => {
                    entity.update(cx, |this, cx| {
                        this.status_message = format!("Error listando SFTP: {e}");
                        this.sftp.remote_loading = false;
                        this.sftp.remote_connected = false;
                        cx.notify();
                    }).ok();
                }
            }
        }).detach();
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

impl gpui::Focusable for AppState {
    fn focus_handle(&self, _cx: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Render principal
// ═══════════════════════════════════════════════════════════════════════════

impl Render for AppState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let decorations = window.window_decorations();
        let is_csd = matches!(decorations, Decorations::Client { .. });
        let win_ctrls = window.window_controls();
        let is_maximized = window.is_maximized();

        let nav = self.nav;
        let collapsed = self.sidebar_collapsed;
        let vok = self.vault_unlocked;
        let ic = collapsed;

        // Colores de botones (Hsla: Copy, capturados por valor en closures)
        let btn_fg       = cx.theme().foreground;
        let btn_hover    = cx.theme().secondary_hover;
        let btn_active   = cx.theme().secondary_active;
        let close_hover  = cx.theme().danger;
        let close_active = cx.theme().danger_active;
        let close_fg     = cx.theme().danger_foreground;

        v_flex().size_full().bg(cx.theme().background)
            .overflow_hidden()
            // Bordes redondeados en ventana flotante, planos al encajar/tiled (estilo Zed, 10px)
            .map(|this| match decorations {
                Decorations::Client { tiling } => this
                    .when(!(tiling.top || tiling.left),    |d| d.rounded_tl(px(10.)))
                    .when(!(tiling.top || tiling.right),   |d| d.rounded_tr(px(10.)))
                    .when(!(tiling.bottom || tiling.left), |d| d.rounded_bl(px(10.)))
                    .when(!(tiling.bottom || tiling.right),|d| d.rounded_br(px(10.))),
                _ => this,
            })
            // ── Titlebar personalizada (estilo Zed): drag + botones CSD circulares ──
            .child(
                h_flex()
                    .id("titlebar")
                    .w_full().h(px(34.)).flex_shrink_0()
                    .pl(px(12.))
                    .border_b_1().border_color(cx.theme().title_bar_border)
                    .bg(cx.theme().title_bar)
                    .window_control_area(WindowControlArea::Drag)
                    // Izquierda: toggle sidebar + nombre
                    .child(h_flex().items_center().gap_2()
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default(); cx.stop_propagation();
                        })
                        .child(SidebarToggleButton::new().collapsed(ic)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar_collapsed = !this.sidebar_collapsed; cx.notify();
                            })))
                        .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("ShellMounter")))
                    // Centro: hosts / versión
                    .child(div().flex_1().flex().items_center().justify_center()
                        .text_xs().text_color(cx.theme().muted_foreground)
                        .child(format!("{} hosts · v{}", self.hosts.len(), env!("CARGO_PKG_VERSION"))))
                    // Derecha: botones de ventana circulares (solo CSD)
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
                        .child(menuitem("Logs", IconName::Inbox, nav == Nav::Logs, cx, |s, cx| { s.nav = Nav::Logs; cx.notify(); }))
                        .child(menuitem("Settings", IconName::Settings, nav == Nav::Settings, cx, |s, cx| { s.nav = Nav::Settings; cx.notify(); }))
                        .child(menuitem("SFTP", IconName::HardDrive, nav == Nav::Sftp, cx, |s, cx| { s.nav = Nav::Sftp; s.load_local_files(); cx.notify(); }))))
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
        Nav::Settings => render_settings_view(state, cx).into_any_element(),
        Nav::Sftp => render_sftp_view(state, cx).into_any_element(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Hosts
// ═══════════════════════════════════════════════════════════════════════════

fn render_hosts_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let query: String = state.search_query.clone().into();
    let query_lower = query.to_lowercase();
    v_flex().flex_1().size_full()
        .child(h_flex().h_12().px_4().gap_2().border_b_1().border_color(cx.theme().border)
            .child(btn("+ Nuevo host", true, cx, |s, cx| { s.modal = Some(Modal::HostEditor); cx.notify(); }))
            // Search bar
            .child(h_flex().flex_1().h_8().px_3().rounded(cx.theme().radius).border_1().border_color(cx.theme().border)
                .bg(cx.theme().secondary).items_center().gap_1())
            // Quick Connect button
            .child(div().h_8().px_3().rounded(cx.theme().radius)
                .bg(cx.theme().primary).text_color(cx.theme().primary_foreground)
                .flex().items_center().text_sm().cursor_pointer()
                .hover(|d| d.bg(cx.theme().primary_hover))
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _: &gpui::MouseDownEvent, _window, cx| {
                    this.modal = Some(Modal::HostEditor);
                    cx.notify();
                }))
                .child("Quick Connect")))
        .child(div().id("host-scroll").flex_1().overflow_y_scrollbar().p_4()
            .children({
                let mut items: Vec<AnyElement> = vec![];
                if state.hosts.is_empty() {
                    items.push(empty("Sin hosts", "Agrega tu primer servidor SSH.", IconName::Network, cx).into_any_element());
                } else {
                    for (gn, hosts) in &state.groups {
                        // Filter by search query
                        let filtered: Vec<&Host> = if query_lower.is_empty() {
                            hosts.iter().collect()
                        } else {
                            hosts.iter().filter(|h| {
                                h.label.to_lowercase().contains(&query_lower) ||
                                h.hostname.to_lowercase().contains(&query_lower) ||
                                h.username.to_lowercase().contains(&query_lower)
                            }).collect()
                        };
                        if filtered.is_empty() { continue; }
                        items.push(v_flex().mb_4().gap_1()
                            .child(div().px_1().mb_1().text_xs().font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().muted_foreground).child(gn.clone()))
                            .children(filtered.iter().map(|h| render_host_card(h, state, cx)))
                            .into_any_element());
                    }
                }
                items
            }))
}

fn render_host_card(host: &Host, state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let hid = host.id.clone(); let hid2 = host.id.clone();
    let hid_edit = host.id.clone(); let hid_del = host.id.clone();
    let lbl = host.label.clone();
    let sel = state.selected_host_id.as_deref() == Some(&host.id);
    let conn = state.tabs.iter().any(|t| t.connected);
    let ac = avatar_color(&host.id);
    let first: SharedString = lbl.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into()).into();

    h_flex().id(format!("host-card-{}", hid)).w_full().px_3().py_2().rounded(cx.theme().radius).gap_3()
        .bg(cx.theme().background).border_1()
        .border_color(if sel { cx.theme().primary } else { cx.theme().border })
        .cursor_pointer().hover(|d| d.bg(cx.theme().accent))
        .on_click(cx.listener(move |this, _, _, cx| { this.connect_host(&hid, cx); }))
        .on_mouse_down(gpui::MouseButton::Right, cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
            this.selected_host_id = Some(hid2.clone());
            cx.notify();
        }))
        .child(div().size_9().rounded(cx.theme().radius).flex().items_center().justify_center()
            .flex_shrink_0().bg(rgb(ac)).text_color(rgb(0xffffff)).font_weight(FontWeight::BOLD).text_sm().child(first))
        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(lbl))
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(format!("ssh  {}@{}:{}", host.username, host.hostname, host.port))))
        .when(conn, |d| d.child(div().size_2().rounded_full().flex_shrink_0().bg(rgb(0x22c55e))))
        .child(h_flex().gap_1().ml_2()
            .child(div().id(format!("edit-host-{}", hid_edit)).px_2().py_1().rounded(cx.theme().radius)
                .bg(cx.theme().secondary).text_xs().cursor_pointer()
                .hover(|d| d.bg(cx.theme().secondary_hover))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.host_form.editing_id = Some(hid_edit.clone());
                    this.modal = Some(Modal::HostEditor);
                    cx.notify();
                }))
                .child("Editar"))
            .child(div().id(format!("del-host-{}", hid_del)).px_2().py_1().rounded(cx.theme().radius)
                .bg(rgb(0xef4444)).text_color(rgb(0xffffff)).text_xs().cursor_pointer()
                .hover(|d| d.bg(rgb(0xdc2626)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.modal = Some(Modal::ConfirmDelete(hid_del.clone()));
                    cx.notify();
                }))
                .child("Eliminar")))
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
        .child(div().flex_1().overflow_y_scrollbar().p_4()
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
        .child(div().flex_1().overflow_y_scrollbar().p_4()
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
        .child(div().flex_1().overflow_y_scrollbar().p_4()
            .children(if snippets.is_empty() {
                vec![empty("Sin snippets", "Guarda comandos frecuentes.", IconName::SquareTerminal, cx).into_any_element()]
            } else {
                snippets.iter().map(|s| {
                    let cmd = s.command.clone();
                    h_flex().id(format!("snip-{}", s.id)).w_full().px_3().py_2().rounded(cx.theme().radius).gap_3().bg(cx.theme().background)
                        .border_1().border_color(cx.theme().border).mb_1().cursor_pointer().hover(|d| d.bg(cx.theme().accent))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            // Send command to active terminal if connected
                            if let Some(tab) = this.tabs.get(this.active_tab) {
                                if let Some(ref sess) = tab.session {
                                    let sess = sess.clone();
                                    let cmd_with_newline = format!("{}\r", cmd);
                                    let data = cmd_with_newline.into_bytes();
                                    cx.spawn(async move |_entity: gpui::WeakEntity<AppState>, _cx| {
                                        let mut s = sess.lock();
                                        let _ = s.send(&data).await;
                                    }).detach();
                                    // Also write to the terminal view for visual feedback
                                    tab.terminal.lock().write(cmd.as_bytes());
                                    tab.terminal.lock().write(b"\r\n");
                                    this.status_message = format!("Sent: {}", &cmd);
                                } else {
                                    this.status_message = "No active terminal connection".into();
                                }
                            }
                            cx.notify();
                        }))
                        .child(Icon::new(IconName::SquareTerminal).small())
                        .child(v_flex().flex_1().overflow_hidden().gap_0p5()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(s.label.clone()))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).font_family("monospace").child(s.command.clone())))
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
        .child(div().flex_1().overflow_y_scrollbar().p_4().font_family("monospace").text_xs().text_color(gpui::rgb(0x9aa3bf))
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
                .on_click(cx.listener(|this, _, _, cx| {
                    this.log_lines = AppState::load_logs(&this.data_dir); cx.notify();
                })).child("Refresh")))
        .child(div().flex_1().overflow_y_scrollbar().p_4().font_family("monospace").text_xs().text_color(cx.theme().muted_foreground)
            .children(state.log_lines.iter().map(|l| div().py_0p5().child(l.clone())).collect::<Vec<_>>()))
}

/// Map a GPUI KeyDownEvent to terminal byte sequence for SSH PTY.
fn key_to_terminal_bytes(event: &gpui::KeyDownEvent) -> Vec<u8> {
    let key = &event.keystroke.key;
    let modifiers = &event.keystroke.modifiers;

    // Ctrl+letter → control character (0x01–0x1A)
    if modifiers.control && key.len() == 1 {
        let c = key.chars().next().unwrap();
        if c.is_ascii_uppercase() || c.is_ascii_lowercase() {
            return vec![c.to_ascii_lowercase() as u8 & 0x1f];
        }
    }

    match key.as_str() {
        "enter" | "return" => vec![b'\r'],
        "backspace" => vec![0x7f],
        "tab" => vec![b'\t'],
        "escape" => vec![0x1b],
        "space" => vec![b' '],
        "up" => vec![0x1b, b'[', b'A'],
        "down" => vec![0x1b, b'[', b'B'],
        "right" => vec![0x1b, b'[', b'C'],
        "left" => vec![0x1b, b'[', b'D'],
        "home" => vec![0x1b, b'[', b'H'],
        "end" => vec![0x1b, b'[', b'F'],
        "delete" => vec![0x1b, b'[', b'3', b'~'],
        "pageup" => vec![0x1b, b'[', b'5', b'~'],
        "pagedown" => vec![0x1b, b'[', b'6', b'~'],
        "f1" => vec![0x1b, b'O', b'P'],
        "f2" => vec![0x1b, b'O', b'Q'],
        "f3" => vec![0x1b, b'O', b'R'],
        "f4" => vec![0x1b, b'O', b'S'],
        "f5" => vec![0x1b, b'[', b'1', b'5', b'~'],
        "f6" => vec![0x1b, b'[', b'1', b'7', b'~'],
        "f7" => vec![0x1b, b'[', b'1', b'8', b'~'],
        "f8" => vec![0x1b, b'[', b'1', b'9', b'~'],
        "f9" => vec![0x1b, b'[', b'2', b'0', b'~'],
        "f10" => vec![0x1b, b'[', b'2', b'1', b'~'],
        "f11" => vec![0x1b, b'[', b'2', b'3', b'~'],
        "f12" => vec![0x1b, b'[', b'2', b'4', b'~'],
        // Printable ASCII — send as-is
        other if other.len() == 1 => {
            let c = other.chars().next().unwrap();
            if c.is_ascii() && !c.is_ascii_control() {
                vec![c as u8]
            } else {
                vec![]
            }
        }
        _ => vec![],
    }
}

fn render_terminal_area(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let bg = gpui::rgb(0x0d1117);
    let fg = gpui::rgb(0xc9d1d9);
    if state.tabs.is_empty() { return div().into_any_element(); }
    let tab = &state.tabs[state.active_tab];
    let terminal = tab.terminal.clone();
    let session = tab.session.clone();
    let tab_id = tab.id.clone();
    let tab_id_key = tab_id.clone();
    let tab_id_scroll = tab_id.clone();
    let tab_id_clip = tab_id.clone();
    let font_size = state.terminal_font_size;
    let active_tab_idx = state.active_tab;
    let num_tabs = state.tabs.len();

    v_flex().flex_1().size_full().bg(bg)
        .child(render_tab_bar(state, cx))
        .child(
            div().id("terminal-canvas").flex_1().overflow_hidden().bg(bg)
                .child(
                    div().size_full()
                        .track_focus(&state.focus_handle)
                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |_this, _event: &gpui::MouseDownEvent, window, cx| {
                            cx.focus_self(window);
                        }))
                        .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _window, cx| {
                    let key = &event.keystroke.key;
                    let mods = &event.keystroke.modifiers;
                    let ctrl = mods.control;
                    let shift = mods.shift;

                    // ── App shortcuts (intercepted, NOT sent to terminal) ──
                    if ctrl && shift {
                        match key.as_str() {
                            "w" => {
                                if this.tabs.len() > 1 {
                                    let idx = this.active_tab;
                                    if let Some(tab) = this.tabs.get(idx) {
                                        if let Some(ref sess) = tab.session {
                                            let sess = sess.clone();
                                            cx.spawn(async move |_entity: gpui::WeakEntity<AppState>, _cx| {
                                                let s = sess.lock();
                                                // Session will be dropped after close
                                                drop(s);
                                            }).detach();
                                        }
                                    }
                                    this.close_tab(this.active_tab, cx);
                                }
                                return;
                            }
                            "tab" => {
                                this.active_tab = (this.active_tab + 1) % this.tabs.len();
                                cx.notify();
                                return;
                            }
                            "c" => {
                                // Copy: get selected text from terminal
                                if let Some(tab) = this.tabs.iter().find(|t| t.id == tab_id) {
                                    let mut t = tab.terminal.lock();
                                    if let Some(text) = t.get_selection_text() {
                                        cx.write_to_clipboard(text.into());
                                        this.status_message = "Copied".into();
                                    }
                                }
                                cx.notify();
                                return;
                            }
                            "v" => {
                                // Paste: read clipboard and send to terminal
                                if let Some(tab) = this.tabs.iter().find(|t| t.id == tab_id) {
                                    if let Some(ref sess) = tab.session {
                                        let text = cx.read_from_clipboard().map(|item| item.text().unwrap_or_default()).unwrap_or_default();
                                        let data = text.into_bytes();
                                        if !data.is_empty() {
                                            let sess = sess.clone();
                                            cx.spawn(async move |_entity: gpui::WeakEntity<AppState>, _cx| {
                                                let mut s = sess.lock();
                                                let _ = s.send(&data).await;
                                            }).detach();
                                        }
                                    }
                                }
                                return;
                            }
                            "n" => {
                                // New connection: switch to hosts view
                                this.nav = Nav::Hosts;
                                cx.notify();
                                return;
                            }
                            _ => {}
                        }
                    }

                    // Ctrl+Plus/Ctrl+Equal → zoom in
                    if ctrl && !shift && (key == "+" || key == "=" || key == "plus") {
                        this.terminal_font_size = (this.terminal_font_size + 1).min(24);
                        cx.notify();
                        return;
                    }
                    // Ctrl+Minus → zoom out
                    if ctrl && !shift && key == "-" {
                        this.terminal_font_size = (this.terminal_font_size - 1).max(8);
                        cx.notify();
                        return;
                    }
                    // Ctrl+0 → reset zoom
                    if ctrl && !shift && key == "0" {
                        this.terminal_font_size = 13;
                        cx.notify();
                        return;
                    }

                    // ── Terminal input (forward to SSH) ──
                    if let Some(tab) = this.tabs.iter().find(|t| t.id == tab_id) {
                        if let Some(ref sess) = tab.session {
                            let bytes = key_to_terminal_bytes(event);
                            if !bytes.is_empty() {
                                let sess = sess.clone();
                                let b = bytes;
                                cx.spawn(async move |_entity: gpui::WeakEntity<AppState>, _cx| {
                                    let mut s = sess.lock();
                                    let _ = s.send(&b).await;
                                }).detach();
                            }
                        }
                    }
                }))
                .on_scroll_wheel(cx.listener(move |this, event: &gpui::ScrollWheelEvent, _window, cx| {
                    // Scroll terminal scrollback
                    let tid = tab_id_scroll.clone();
                    if let Some(tab) = this.tabs.iter().find(|t| t.id == tid) {
                        let mut t = tab.terminal.lock();
                        let delta = match event.delta {
                            gpui::ScrollDelta::Pixels(pos) => pos.y.into(),
                            gpui::ScrollDelta::Lines(lines) => lines.y * 18.0,
                        };
                        if delta > 0.0 {
                            t.scroll((delta / 18.0).ceil() as isize); // ~18px per line
                        } else if delta < 0.0 {
                            t.scroll((delta / 18.0).floor() as isize);
                        }
                    }
                    cx.notify();
                }))
                .child({
                    let lines = {
                        let mut t = terminal.lock();
                        t.visible_lines().0
                    };
                    let fs = font_size;
                    v_flex().gap_0().p_2().font_family("monospace")
                        .text_size(px(fs as f32)).text_color(fg)
                        .children(lines.into_iter().map(|line| {
                            div().h(px((fs + 4) as f32)).child(if line.is_empty() { gpui::SharedString::from(" ") } else { gpui::SharedString::from(line.as_str()) })
                        }))
                })
                )
        )
        .into_any_element()
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
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| { this.close_tab(ti, cx); }))
                    .on_mouse_down(gpui::MouseButton::Middle, cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                        this.close_tab(ti, cx);
                    }))
                    .child("\u{00D7}"))
        }))
}

// ═══════════════════════════════════════════════════════════════════════════
// Settings
// ═══════════════════════════════════════════════════════════════════════════

fn render_settings_view(_state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    use gpui_component::{Theme, ThemeRegistry};
    let current = Theme::global(cx).theme_name().clone();
    let themes: Vec<_> = ThemeRegistry::global(cx).sorted_themes().into_iter().cloned().collect();

    v_flex().flex_1().size_full().p_4().gap_2()
        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).mb_2().child("Themes"))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).mb_2()
            .child(format!("{} themes available", themes.len())))
        .child(div().flex_1().overflow_hidden()
            .child(v_flex().gap_1().overflow_y_scrollbar()
                .children(themes.iter().map(|theme| {
                    let is_active = theme.name.as_ref() == current.as_ref();
                    let name = theme.name.to_string();
                    let mode_label = if theme.mode.is_dark() { "dark" } else { "light" };
                    let mode_is_dark = theme.mode.is_dark();
                    let theme_clone = (*theme).clone();
                    let primary_bg = cx.theme().primary;
                    let primary_fg = cx.theme().primary_foreground;
                    let muted = cx.theme().muted_foreground;
                    let radius = cx.theme().radius;
                    h_flex().id(ElementId::Name(format!("theme-{}", name).into()))
                        .px_3().py_2().rounded(radius)
                        .gap_2().items_center().cursor_pointer()
                        .bg(if is_active { primary_bg } else { cx.theme().secondary })
                        .text_color(if is_active { primary_fg } else { cx.theme().foreground })
                        .hover(|d| d.bg(if is_active { primary_bg } else { cx.theme().secondary }))
                        .on_click(cx.listener(move |_this, _, _, cx| {
                            Theme::global_mut(cx).apply_config(&theme_clone);
                            cx.notify();
                        }))
                        .child(div().size_2().rounded_full().flex_shrink_0()
                            .bg(if is_active { primary_fg } else {
                                if mode_is_dark { hsla(0.664, 0.866, 0.5, 1.0) } else { hsla(0.123, 0.824, 0.427, 1.0) }
                            }))
                        .child(div().flex_1().text_sm().child(name))
                        .child(div().text_xs().text_color(if is_active { primary_fg } else { muted }).child(mode_label))
                }))))
}

// ═══════════════════════════════════════════════════════════════════════════
// SFTP Dual-Pane File Browser
// ═══════════════════════════════════════════════════════════════════════════

fn render_sftp_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let sftp = &state.sftp;
    let local_entries = &sftp.local_entries;
    let host_list = &state.hosts;
    let selected = sftp.selected_host_id.clone();
    let remote_connected = sftp.remote_connected;
    let remote_entries_clone = sftp.remote_entries.clone();
    let remote_path_display = sftp.remote_path.clone();
    let remote_loading = sftp.remote_loading;
    let local_path_display = sftp.local_path.clone();

    v_flex().flex_1().size_full()
        // Toolbar
        .child(h_flex().h_10().px_3().gap_2().items_center().border_b_1().border_color(cx.theme().border)
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("SFTP Browser"))
            .child(div().flex_1())
            // Hidden files toggle
            .child(h_flex().gap_1().items_center().px_2().py_1().rounded(cx.theme().radius)
                .id("toggle-hidden")
                .bg(cx.theme().secondary).text_xs().text_color(cx.theme().muted_foreground).cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.sftp.show_hidden = !this.sftp.show_hidden;
                    this.load_local_files();
                    cx.notify();
                }))
                .child(if sftp.show_hidden { "Hide hidden" } else { "Show hidden" })))
        // Dual pane
        .child(h_flex().flex_1().min_h_0()
            // Left pane — Local
            .child(v_flex().flex_1().min_w_0().border_r_1().border_color(cx.theme().border)
                .child(h_flex().h_8().px_2().gap_1().items_center().border_b_1().border_color(cx.theme().border).bg(cx.theme().secondary)
                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().muted_foreground).child("Local"))
                    .child(div().flex_1())
                    .child(div().text_xs().text_color(cx.theme().muted_foreground)
                        .child(local_path_display.clone())))
                .child(div().flex_1().overflow_hidden()
                    .child(v_flex().gap_0().overflow_y_scrollbar().h_full()
                        // Parent dir
                        .child(render_file_row("..", "", true, cx, |this, cx| {
                            let parent = std::path::Path::new(&this.sftp.local_path)
                                .parent().map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|| "/".into());
                            this.sftp.local_path = parent;
                            this.load_local_files();
                            cx.notify();
                        }))
                        .children(local_entries.iter().map(|entry| {
                            let name = entry.name.clone();
                            let is_dir = entry.is_dir;
                            let size = crate::fs::format_size(entry.size);
                            let entry_path = entry.path.clone();
                            render_file_row(&name, &size, is_dir, cx, move |this, cx| {
                                if is_dir {
                                    this.sftp.local_path = entry_path.clone();
                                    this.load_local_files();
                                    cx.notify();
                                }
                            })
                        }))))))
            // Right pane — Remote host picker or file list
            .child(v_flex().flex_1().min_w_0()
                .child(h_flex().h_8().px_2().gap_1().items_center().border_b_1().border_color(cx.theme().border).bg(cx.theme().secondary)
                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().muted_foreground).child("Remote"))
                    .child(div().flex_1())
                    .child(div().text_xs().text_color(cx.theme().muted_foreground)
                        .child(if remote_connected { remote_path_display.clone() } else { selected.clone().unwrap_or_else(|| "Select a host".into()) })))
                .child(div().flex_1().overflow_hidden()
                    .child(v_flex().gap_0().overflow_y_scrollbar().h_full()
                        .when(remote_loading, |d| d.child(div().p_4().text_xs().text_color(cx.theme().muted_foreground).child("Conectando...")))
                        .when(remote_connected, |d| {
                            let mut children: Vec<AnyElement> = vec![
                                // Parent dir
                                render_file_row("..", "", true, cx, move |this, cx| {
                                    if let Some(ref sftp) = this.sftp.sftp_session.clone() {
                                        let sftp = sftp.clone();
                                        let current = this.sftp.remote_path.clone();
                                        let parent = std::path::Path::new(&current)
                                            .parent().map(|p| p.to_string_lossy().to_string())
                                            .unwrap_or_else(|| "/".into());
                                        let hostname2 = String::new(); // dummy
                                        cx.spawn(async move |_entity: gpui::WeakEntity<AppState>, _cx| {
                                            if let Ok(entries) = crate::ssh::sftp::list(&sftp.lock(), &parent).await {
                                                let _ = entries;
                                            }
                                        }).detach();
                                    }
                                }).into_any_element(),
                            ];
                            for entry in &remote_entries_clone {
                                let name = entry.name.clone();
                                let is_dir = entry.is_dir;
                                let size = crate::fs::format_size(entry.size);
                                let entry_path = entry.path.clone();
                                children.push(render_file_row(&name, &size, is_dir, cx, move |this, cx| {
                                    if is_dir {
                                        if let Some(ref sftp) = this.sftp.sftp_session.clone() {
                                            let sftp2 = sftp.clone();
                                            let path2 = entry_path.clone();
                                            this.sftp.remote_path = path2.clone();
                                            this.sftp.remote_loading = true;
                                            cx.notify();
                                            cx.spawn(async move |entity: gpui::WeakEntity<AppState>, cx| {
                                                let result = crate::ssh::sftp::list(&sftp2.lock(), &path2).await;
                                                entity.update(cx, |this, cx| {
                                                    this.sftp.remote_loading = false;
                                                    match result {
                                                        Ok(entries) => {
                                                            this.sftp.remote_entries = entries.into_iter().map(|e| {
                                                                crate::fs::FileEntry {
                                                                    name: e.name.clone(),
                                                                    path: format!("{}/{}", path2.trim_end_matches('/'), e.name),
                                                                    is_dir: e.is_dir,
                                                                    size: e.size.unwrap_or(0),
                                                                    modified: String::new(),
                                                                }
                                                            }).collect();
                                                        }
                                                        Err(e) => {
                                                            this.status_message = format!("Error SFTP: {e}");
                                                        }
                                                    }
                                                    cx.notify();
                                                }).ok();
                                            }).detach();
                                        }
                                    }
                                }).into_any_element());
                            }
                            d.children(children)
                        })
                        .when(!remote_connected && !remote_loading, |d| {
                            d.children(host_list.iter().map(|host| {
                                render_host_item_sftp(host, selected.clone(), cx)
                            }))
                        }))))
}

fn render_host_item_sftp(host: &Host, selected: Option<String>, cx: &mut Context<AppState>) -> impl IntoElement {
    let hid = host.id.clone();
    let hlabel = host.label.clone();
    let hname = host.hostname.clone();
    let is_sel = selected.as_deref() == Some(hid.as_str());
    let first_char = host.label.chars().next().unwrap_or('?').to_string();
    let elem_id = format!("sftp-host-{}", hid);
    h_flex().id(ElementId::Name(elem_id.into())).px_3().py_2().gap_2().items_center().cursor_pointer()
        .bg(if is_sel { cx.theme().primary } else { cx.theme().background })
        .text_color(if is_sel { cx.theme().primary_foreground } else { cx.theme().foreground })
        .hover(|d| if !is_sel { d.bg(cx.theme().secondary) } else { d })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.connect_sftp_host(&hid, cx);
        }))
        .child(div().size_5().rounded(cx.theme().radius).bg(rgb(avatar_color(&host.id))).flex().items_center().justify_center()
            .child(div().text_xs().text_color(rgb(0xffffff)).child(first_char)))
        .child(v_flex().flex_1().min_w_0()
            .child(div().text_sm().child(hlabel))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(hname)))
}
fn render_host_item(host: &Host, selected: Option<String>, cx: &mut Context<AppState>) -> impl IntoElement {
    let hid = host.id.clone();
    let hlabel = host.label.clone();
    let hname = host.hostname.clone();
    let is_sel = selected.as_deref() == Some(hid.as_str());
    let first_char = host.label.chars().next().unwrap_or('?').to_string();
    let elem_id = format!("host-{}", hid);
    h_flex().id(ElementId::Name(elem_id.into())).px_3().py_2().gap_2().items_center().cursor_pointer()
        .bg(if is_sel { cx.theme().primary } else { cx.theme().background })
        .text_color(if is_sel { cx.theme().primary_foreground } else { cx.theme().foreground })
        .hover(|d| if !is_sel { d.bg(cx.theme().secondary) } else { d })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.sftp.selected_host_id = Some(hid.clone());
            cx.notify();
        }))
        .child(div().size_5().rounded(cx.theme().radius).bg(rgb(avatar_color(&host.id))).flex().items_center().justify_center()
            .child(div().text_xs().text_color(rgb(0xffffff)).child(first_char)))
        .child(v_flex().flex_1().min_w_0()
            .child(div().text_sm().child(hlabel))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(hname)))
}

fn render_file_row(
    name: &str, size: &str, is_dir: bool,
    cx: &mut Context<AppState>,
    on_click: impl Fn(&mut AppState, &mut Context<AppState>) + 'static,
) -> impl IntoElement {
    let name_owned = name.to_string();
    let size_owned = size.to_string();
    let icon_color = if is_dir { hsla(0.583, 0.891, 0.58, 1.0) } else { cx.theme().muted_foreground };
    h_flex().id(ElementId::Name(format!("file-{}", name_owned).into()))
        .px_2().py_1().gap_2().items_center().cursor_pointer()
        .hover(|d| d.bg(cx.theme().secondary))
        .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
        .child(Icon::new(if is_dir { IconName::Folder } else { IconName::File }).small().text_color(icon_color))
        .child(div().flex_1().min_w_0().text_sm().child(name_owned.clone()))
        .child(div().w(px(70.)).text_xs().text_color(cx.theme().muted_foreground).child(size_owned.clone()))
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
                .child(div().id("btn-close-modal").size_6().rounded(cx.theme().radius).flex().items_center().justify_center()
                    .bg(cx.theme().secondary).cursor_pointer().hover(|d| d.bg(cx.theme().secondary_hover))
                    .on_click(cx.listener(|this, _, _, cx| { this.modal = None; cx.notify(); }))
                    .child(Icon::new(IconName::Close).small())))
            .child(body))
}

fn render_host_form(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().gap_3()
        .child(Input::new(&state.host_form.label))
        .child(Input::new(&state.host_form.hostname))
        .child(h_flex().gap_3()
            .child(div().flex_1().child(Input::new(&state.host_form.username)))
            .child(div().w(px(80.)).child(Input::new(&state.host_form.port))))
        .child(Input::new(&state.host_form.group))
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
                    v_flex().gap_0p5().max_h(px(160.)).overflow_y_scrollbar().border_1().border_color(cx.theme().border)
                        .rounded(cx.theme().radius)
                        .children(state.available_keys.iter().map(|k| {
                            let fp = k.fingerprint.clone();
                            let sel = state.host_form.selected_key_id.as_deref() == Some(&fp);
                            h_flex().id(format!("key-{}", &fp[..8])).px_3().py_2().gap_2().cursor_pointer()
                                .bg(if sel { cx.theme().accent } else { cx.theme().background })
                                .hover(|d| d.bg(cx.theme().accent))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.host_form.selected_key_id = Some(fp.clone()); cx.notify();
                                }))
                                .child(Icon::new(IconName::HardDrive).small())
                                .child(v_flex().gap_0p5()
                                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(cx.theme().foreground).child(k.label.clone()))
                                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(k.fingerprint[..20.min(k.fingerprint.len())].to_string())))
                                .into_any_element()
                        }))
                )))
        })
        .when(state.host_form.auth_type == "password", |d| {
            d.child(Input::new(&state.host_form.password))
        })
        .child(h_flex().gap_2().justify_end().pt_1()
            .child(btn("Cancelar", false, cx, |s, cx| { s.modal = None; cx.notify(); }))
            .child(btn("Guardar", true, cx, |s, cx| { s.save_host(cx); })))
}

fn render_key_gen_form(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().gap_3()
        .child(Input::new(&state.key_gen_form.label))
        .child(h_flex().gap_2()
            .child(toggle("Ed25519", &state.key_gen_form.key_type == "ed25519", cx, |s, cx| { s.key_gen_form.key_type = "ed25519".into(); cx.notify(); }))
            .child(toggle("ECDSA P-256", &state.key_gen_form.key_type == "ecdsa-p256", cx, |s, cx| { s.key_gen_form.key_type = "ecdsa-p256".into(); cx.notify(); })))
        .child(Input::new(&state.key_gen_form.passphrase))
        .child(h_flex().gap_2().justify_end().pt_1()
            .child(btn("Cancelar", false, cx, |s, cx| { s.modal = None; cx.notify(); }))
            .child(btn("Generar", true, cx, |s, cx| { s.generate_key(cx); })))
}

fn render_vault_unlock_form(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    v_flex().gap_3()
        .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Ingresa la contraseña del vault para desbloquear tus keys SSH."))
        .child(Input::new(&state.vault_password))
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
            .child(div().id("btn-delete-confirm").px_4().py_2().rounded(cx.theme().radius).bg(rgb(0xef4444)).text_color(rgb(0xffffff))
                .text_sm().font_weight(FontWeight::MEDIUM).cursor_pointer().hover(|d| d.bg(rgb(0xdc2626)))
                .on_click(cx.listener(move |this, _, _, cx| { this.delete_host(&id2, cx); }))
                .child("Eliminar")))
}

// ═══════════════════════════════════════════════════════════════════════════
// Componentes reutilizables
// ═══════════════════════════════════════════════════════════════════════════

fn btn(label: &str, primary: bool, cx: &mut Context<AppState>,
       f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static) -> impl IntoElement {
    let id = format!("btn-{}", label.to_lowercase().replace(' ', "-"));
    let lbl = label.to_string();
    div().id(id).h_8().px_3()
        .rounded(cx.theme().radius).flex().items_center().gap_1().text_sm()
        .font_weight(FontWeight::MEDIUM).cursor_pointer()
        .when(primary, |d| d.bg(cx.theme().primary).text_color(cx.theme().primary_foreground)
              .hover(|d| d.bg(cx.theme().primary_hover)))
        .when(!primary, |d| d.bg(cx.theme().secondary).border_1().border_color(cx.theme().border)
              .hover(|d| d.bg(cx.theme().secondary_hover)))
        .child(lbl).on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
}

fn toggle(label: &str, active: bool, cx: &mut Context<AppState>,
          f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static) -> impl IntoElement {
    let lbl = label.to_string();
    div().id(format!("tgl-{}", label.to_lowercase())).flex_1().h_9().rounded(cx.theme().radius).flex().items_center().justify_center()
        .text_sm().font_weight(FontWeight::MEDIUM).cursor_pointer()
        .bg(if active { cx.theme().primary } else { cx.theme().secondary })
        .text_color(if active { cx.theme().primary_foreground } else { cx.theme().foreground })
        .child(lbl).on_click(cx.listener(move |this, _, _, cx| f(this, cx)))
}

fn empty(title: &str, desc: &str, icon: IconName, cx: &mut Context<AppState>) -> impl IntoElement {
    let t = title.to_string();
    let d = desc.to_string();
    v_flex().size_full().items_center().justify_center().gap_2().pt_16()
        .child(Icon::new(icon).large())
        .child(div().font_weight(FontWeight::SEMIBOLD).text_base().child(t))
        .child(div().text_sm().text_color(cx.theme().muted_foreground).child(d))
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
