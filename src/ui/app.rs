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

use anyhow::Context as _;

use crate::db::hosts::{AuthMethod, Host, HostDb};
use crate::ssh::keys::{self, KeyType, SshKey};
use crate::ssh::port_forward::{ForwardKind, PortForwardManager, PortForwardRule};
use crate::ssh::session::{AuthMethod as SshAuth, SshSession};
use crate::ssh::snippets::{Snippet, SnippetStore};
use crate::vault::store::{SecretKind, Vault};

use crate::ai::chat::{ChatState, ChatStatus};
use crate::ai::ui::chat_view::render_chat_view;
use crate::ai::ui::input_bar::{render_input_bar, InputMode};
use crate::ai::ui::mini_window::render_mini_window;
use crate::ai::agent::AgentRunner;
use crate::ai::providers::openai::OpenAiProvider;

// ═══════════════════════════════════════════════════════════════════════════
// Colores de avatar
// ═══════════════════════════════════════════════════════════════════════════

const AC: [u32; 6] = [0xef4444, 0x6366f1, 0x22c55e, 0xa855f7, 0xf97316, 0x0ea5e9];

// ═══════════════════════════════════════════════════════════════════════════
// Navegación
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum HostViewMode { List, #[default] Grid }

#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum Nav {
    #[default] Hosts,
    Terminal, Keychain, PortForwarding, Snippets, KnownHosts, Logs, Settings, Sftp,
    Termia, Git,
}

#[derive(Clone, PartialEq)]
pub(crate) enum Modal { HostEditor, KeyGen, VaultUnlock, ConfirmDelete(String) }

// ═══════════════════════════════════════════════════════════════════════════
// Estado de la app
// ═══════════════════════════════════════════════════════════════════════════

pub(crate) struct AppState {
    pub(crate) host_db: Arc<HostDb>,
    pub(crate) vault: Arc<parking_lot::Mutex<Vault>>,
    pub(crate) snippet_store: Option<SnippetStore>,
    /// Snippet editor fields.
    pub(crate) snippet_label: Entity<InputState>,
    pub(crate) snippet_command: Entity<InputState>,
    pub(crate) port_forward: PortForwardManager,
    pub(crate) data_dir: PathBuf,

    pub(crate) nav: Nav,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) tabs: Vec<TabState>,
    pub(crate) active_tab: usize,
    pub(crate) selected_host_id: Option<String>,
    pub(crate) hosts: Vec<Host>,
    pub(crate) groups: Vec<(String, Vec<Host>)>,
    pub(crate) vault_unlocked: bool,
    pub(crate) modal: Option<Modal>,
    pub(crate) status_message: String,
    pub(crate) host_form: HostForm,
    pub(crate) key_gen_form: KeyGenForm,
    pub(crate) vault_password: Entity<InputState>,
    pub(crate) available_keys: Vec<SshKey>,
    pub(crate) known_host_entries: Vec<String>,
    pub(crate) log_lines: Vec<String>,
    // SFTP file browser state
    pub(crate) sftp: SftpState,
    // Host search filter
    /// Search input entity.
    pub(crate) search_input: Entity<InputState>,
    /// Focus handle for terminal keyboard input.
    pub(crate) focus_handle: FocusHandle,
    /// Terminal font size in px (default 13, range 8-24).
    pub(crate) terminal_font_size: usize,
    /// Host view mode: grid or list.
    pub(crate) host_view_mode: HostViewMode,
    // Port forwarding form
    pub(crate) pf_label: Entity<InputState>,
    pub(crate) pf_local_port: Entity<InputState>,
    pub(crate) pf_remote_host: Entity<InputState>,
    pub(crate) pf_remote_port: Entity<InputState>,
    pub(crate) pf_kind: String,
    // Command broadcast: selected host IDs for multi-exec
    pub(crate) broadcast_selected: std::collections::HashSet<String>,
    // Current terminal font family
    pub(crate) terminal_font_family: String,
    // Termia AI
    pub(crate) chat_state: ChatState,
    pub(crate) ai_mini_visible: bool,
    pub(crate) ai_api_key: String,
    pub(crate) ai_model: String,
}

#[derive(Clone)]
pub(crate) struct SftpState {
    pub(crate) local_path: String,
    pub(crate) local_entries: Vec<crate::fs::FileEntry>,
    pub(crate) local_loading: bool,
    pub(crate) show_hidden: bool,
    pub(crate) selected_host_id: Option<String>,
    // Remote SFTP state
    pub(crate) remote_path: String,
    pub(crate) remote_entries: Vec<crate::fs::FileEntry>,
    pub(crate) remote_loading: bool,
    pub(crate) remote_connected: bool,
    /// Active SFTP session (wrapped for async sharing).
    pub(crate) sftp_session: Option<std::sync::Arc<parking_lot::Mutex<russh_sftp::client::SftpSession>>>,
    /// SSH session handle (needed to keep the connection alive).
    pub(crate) ssh_session: Option<std::sync::Arc<parking_lot::Mutex<SshSession>>>,
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
pub(crate) struct TabState {
    pub(crate) id: String, pub(crate) host_label: String, pub(crate) connected: bool,
    /// Terminal emulator — wrapped in Arc for sharing with SSH recv task.
    pub(crate) terminal: std::sync::Arc<parking_lot::Mutex<crate::terminal::view::TerminalView>>,
    /// Active SSH session (present when connected).
    pub(crate) session: Option<std::sync::Arc<parking_lot::Mutex<SshSession>>>,
    /// Split pane layout for this tab.
    pub(crate) layout: crate::terminal::split::TerminalLayout,
}

impl TabState {
    fn new(id: String, host_label: String) -> Self {
        let term = crate::terminal::view::TerminalView::new(
            crate::terminal::view::TerminalSize::new(120, 40),
        );
        let mut layout = crate::terminal::split::TerminalLayout::default();
        // Set the root pane to match this tab
        if let crate::terminal::split::TerminalPane::Leaf { ref mut host_label, .. } = layout.root {
            *host_label = id.clone();
        }
        Self {
            id, host_label, connected: false,
            terminal: std::sync::Arc::new(parking_lot::Mutex::new(term)),
            session: None,
            layout,
        }
    }
}

#[derive(Clone)]
pub(crate) struct HostForm {
    pub(crate) label: Entity<InputState>, pub(crate) hostname: Entity<InputState>, pub(crate) port: Entity<InputState>,
    pub(crate) username: Entity<InputState>, pub(crate) group: Entity<InputState>, pub(crate) password: Entity<InputState>,
    pub(crate) auth_type: String,
    pub(crate) selected_key_id: Option<String>,
    pub(crate) editing_id: Option<String>,
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
pub(crate) struct KeyGenForm {
    pub(crate) label: Entity<InputState>, pub(crate) passphrase: Entity<InputState>,
    pub(crate) key_type: String,
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
            snippet_label: cx.new(|cx| InputState::new(window, cx).placeholder("Label")),
            snippet_command: cx.new(|cx| InputState::new(window, cx).placeholder("Comando")),
            vault_password: cx.new(|cx| InputState::new(window, cx)),
            available_keys: vec![],
            known_host_entries: known, log_lines: logs,
            sftp: SftpState::default(),
            search_input: cx.new(|cx| InputState::new(window, cx).placeholder("Buscar por IP o label...")),
            focus_handle: cx.focus_handle(),
            terminal_font_size: 13,
            host_view_mode: HostViewMode::Grid,
            pf_label: cx.new(|cx| InputState::new(window, cx).placeholder("Label")),
            pf_local_port: cx.new(|cx| InputState::new(window, cx).placeholder("8080").default_value("8080")),
            pf_remote_host: cx.new(|cx| InputState::new(window, cx).placeholder("localhost")),
            pf_remote_port: cx.new(|cx| InputState::new(window, cx).placeholder("80").default_value("80")),
            pf_kind: "Local".into(),
            broadcast_selected: Default::default(),
            terminal_font_family: "monospace".into(),
            chat_state: ChatState::new(),
            ai_mini_visible: false,
            ai_api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            ai_model: std::env::var("TERMIA_MODEL").unwrap_or_else(|_| "gpt-4o".into()),
        };
        if vok { s.load_keys(); }
        s.restore_session();
        if !s.tabs.is_empty() { s.nav = Nav::Terminal; s.active_tab = 0; }
        s
    }

    pub(crate) fn load_local_files(&mut self) {
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

    pub(crate) fn load_logs(data_dir: &Path) -> Vec<String> {
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

    /// Group hosts into a flat list with hierarchical prefixes.
    /// Groups with '/' create nested virtual folders (e.g. "Prod/US" -> "Prod" then "Prod/US").
    pub(crate) fn group_hosts(hosts: &[Host]) -> Vec<(String, Vec<Host>)> {
        let mut map: std::collections::BTreeMap<String, Vec<Host>> = Default::default();
        let mut u = vec![];
        for h in hosts {
            if let Some(ref g) = h.group_name {
                // Split hierarchical groups: "Prod/US" -> entries for "Prod" + "Prod/US"
                let parts: Vec<&str> = g.split('/').collect();
                if parts.len() > 1 {
                    // Add to each level: "Prod", "Prod/US"
                    let mut prefix = String::new();
                    for part in parts {
                        if !prefix.is_empty() { prefix.push('/'); }
                        prefix.push_str(part);
                        map.entry(prefix.clone()).or_default().push(h.clone());
                    }
                } else {
                    map.entry(g.clone()).or_default().push(h.clone());
                }
            } else { u.push(h.clone()); }
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

    pub(crate) fn unlock_vault(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn save_host(&mut self, cx: &mut Context<Self>) {
        let editing = self.host_form.editing_id.take();
        let id = editing.unwrap_or_else(|| Uuid::new_v4().to_string());
        let label = self.host_form.label.read(cx).value().to_string();
        let hostname = self.host_form.hostname.read(cx).value().to_string();
        let username = self.host_form.username.read(cx).value().to_string();
        let port: u16 = self.host_form.port.read(cx).value().parse().unwrap_or(22);
        let group = self.host_form.group.read(cx).value().to_string();
        let auth_method = match self.host_form.auth_type.as_str() {
            "password" => {
                if !self.vault_unlocked {
                    self.status_message = "Desbloquea el vault primero".into();
                    cx.notify();
                    return;
                }
                let vault_id = Uuid::new_v4().to_string();
                let pw = self.host_form.password.read(cx).value().to_string();
                {
                    let mut vault = self.vault.lock();
                    if let Err(e) = vault.put(&vault_id, "", SecretKind::Password, pw.as_bytes()) {
                        self.status_message = format!("Error guardando password: {e}");
                        cx.notify();
                        return;
                    }
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

    pub(crate) fn delete_host(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.host_db.delete_host(id).is_ok() {
            self.refresh_hosts();
            self.status_message = "Host eliminado".into();
        }
        self.modal = None;
        cx.notify();
    }

    pub(crate) fn generate_key(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn import_key(&mut self, path: &Path, cx: &mut Context<Self>) {
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

    pub(crate) fn connect_host(&mut self, host_id: &str, cx: &mut Context<Self>) {
        if !self.vault_unlocked {
            self.modal = Some(Modal::VaultUnlock);
            self.status_message = "Desbloquea el vault para conectar".into();
            cx.notify();
            return;
        }
        if let Some(host) = self.hosts.iter().find(|h| h.id == host_id).cloned() {
            // Always create a new tab with unique ID — allows multiple sessions to same host
            let unique_tab_id = Uuid::new_v4().to_string();
            let tab = TabState::new(unique_tab_id.clone(), host.label.clone());
            self.tabs.push(tab);
            let tab_idx = self.tabs.len() - 1;
            self.active_tab = tab_idx;
            self.nav = Nav::Terminal;
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
                let result = TOKIO_RT.handle().block_on(async {
                    log::info!("[ssh] conectando a {}:{}", hostname, port);
                    let mut session = SshSession::connect(&hostname, port, &username, auth, &data_dir).await?;
                    log::info!("[ssh] conectado, pidiendo PTY");
                    session.request_pty("xterm-256color", 120, 40).await?;
                    log::info!("[ssh] PTY ok, iniciando shell");
                    session.request_shell().await?;
                    log::info!("[ssh] shell iniciado");
                    Ok::<_, anyhow::Error>(session)
                });
                match result {
                    Ok(session) => {
                        let session = std::sync::Arc::new(parking_lot::Mutex::new(session));

                        // Store session in tab and notify UI
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
                                    TOKIO_RT.handle().block_on(async {
                                        tokio::time::timeout(
                                            std::time::Duration::from_millis(100),
                                            s.recv(),
                                        ).await
                                    })
                                };
                                match data {
                                    Ok(Ok(Some(bytes))) => {
                                        let mut t = term.lock();
                                        t.write(&bytes);
                                    }
                                    // EOF o canal cerrado — salir
                                    Ok(Ok(None)) => {
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
                                    // Timeout — comprobar si la sesión sigue abierta
                                    _ => {
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

    pub(crate) fn close_tab(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.tabs.len() {
            self.tabs.remove(idx);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.active_tab.saturating_sub(1);
            }
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

    pub(crate) fn refresh_hosts(&mut self) {
        if let Ok(hosts) = self.host_db.list_hosts(None) {
            self.groups = Self::group_hosts(&hosts);
            self.hosts = hosts;
        }
    }

    /// Connect to a host for SFTP browsing.
    pub(crate) fn connect_sftp_host(&mut self, host_id: &str, cx: &mut Context<Self>) {
        if !self.vault_unlocked {
            self.modal = Some(Modal::VaultUnlock);
            self.status_message = "Desbloquea el vault para conectar".into();
            cx.notify();
            return;
        }
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
            let ssh = match TOKIO_RT.handle().block_on(async {
                SshSession::connect(&hostname, port, &username, auth, &data_dir).await
            }) {
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

    pub(crate) fn save_snippet(&mut self, snippet: &Snippet, cx: &mut Context<Self>) {
        if let Some(ref store) = self.snippet_store {
            match store.save(snippet) {
                Ok(()) => self.status_message = "Snippet guardado".into(),
                Err(e) => self.status_message = format!("Error: {e}"),
            }
            cx.notify();
        }
    }

    pub(crate) fn delete_snippet(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(ref store) = self.snippet_store {
            match store.delete(id) {
                Ok(()) => self.status_message = "Snippet eliminado".into(),
                Err(e) => self.status_message = format!("Error: {e}"),
            }
            cx.notify();
        }
    }

    /// Save open tabs to a JSON file for session restore.
    /// Send a command to all selected broadcast hosts.
    pub(crate) fn broadcast_command(&mut self, command: &str, cx: &mut Context<Self>) {
        if self.broadcast_selected.is_empty() {
            self.status_message = "Selecciona hosts para broadcast".into();
            cx.notify();
            return;
        }
        let cmd_bytes = format!("{}
", command).into_bytes();
        for host_id in self.broadcast_selected.clone() {
            // Find tab that has this host connected
            if let Some(tab) = self.tabs.iter().find(|t| {
                self.hosts.iter().any(|h| h.id == host_id && h.label == t.host_label)
            }) {
                if let Some(ref sess) = tab.session {
                    let sess = sess.clone();
                    let data = cmd_bytes.clone();
                    cx.spawn(async move |_entity: gpui::WeakEntity<AppState>, _cx| {
                        let mut s = sess.lock();
                        let _ = s.send(&data).await;
                    }).detach();
                    tab.terminal.lock().write(&cmd_bytes);
                }
            }
        }
        self.status_message = format!("Broadcast a {} hosts", self.broadcast_selected.len());
        cx.notify();
    }

    pub(crate) fn save_session(&self) {
        let path = self.data_dir.join("session.json");
        let data: Vec<serde_json::Value> = self.tabs.iter().map(|t| {
            serde_json::json!({
                "id": t.id,
                "host_label": t.host_label,
                "connected": t.connected,
            })
        }).collect();
        if let Ok(json) = serde_json::to_string_pretty(&data) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Restore session from saved JSON file.
    pub(crate) fn restore_session(&mut self) {
        let path = self.data_dir.join("session.json");
        if let Ok(json) = std::fs::read_to_string(&path) {
            if let Ok(data) = serde_json::from_str::<Vec<serde_json::Value>>(&json) {
                for entry in data {
                    let host_label = entry["host_label"].as_str().unwrap_or("unknown").to_string();
                    let tab = TabState::new(Uuid::new_v4().to_string(), host_label);
                    self.tabs.push(tab);
                }
            }
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
        use crate::ui::views::*;
        use crate::ui::views::widgets::status_dot;
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
                        .child(widgets::menuitem("Hosts", IconName::Network, nav == Nav::Hosts, cx, |s, cx| { s.nav = Nav::Hosts; cx.notify(); }))
                        .child(widgets::menuitem("Keychain", IconName::HardDrive, nav == Nav::Keychain, cx, |s, cx| { s.nav = Nav::Keychain; s.load_keys(); cx.notify(); }))
                        .child(widgets::menuitem("Port Fwd", IconName::Network, nav == Nav::PortForwarding, cx, |s, cx| { s.nav = Nav::PortForwarding; cx.notify(); }))
                        .child(widgets::menuitem("Snippets", IconName::SquareTerminal, nav == Nav::Snippets, cx, |s, cx| { s.nav = Nav::Snippets; cx.notify(); }))
                        .child(widgets::menuitem("Known Hosts", IconName::Globe, nav == Nav::KnownHosts, cx, |s, cx| { s.nav = Nav::KnownHosts; cx.notify(); }))
                        .child(widgets::menuitem("Logs", IconName::Inbox, nav == Nav::Logs, cx, |s, cx| { s.nav = Nav::Logs; cx.notify(); }))
                        .child(widgets::menuitem("Settings", IconName::Settings, nav == Nav::Settings, cx, |s, cx| { s.nav = Nav::Settings; cx.notify(); }))
                        .child(widgets::menuitem("SFTP", IconName::HardDrive, nav == Nav::Sftp, cx, |s, cx| { s.nav = Nav::Sftp; s.load_local_files(); cx.notify(); }))
                        .child(widgets::menuitem("Termia", IconName::Search, nav == Nav::Termia, cx, |s, cx| { s.nav = Nav::Termia; cx.notify(); }))
                        .child(widgets::menuitem("Git", IconName::Globe, nav == Nav::Git, cx, |s, cx| { s.nav = Nav::Git; cx.notify(); }))))
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
                    .when(!self.tabs.is_empty(), |d| d.child(terminal::render_tab_bar(self, cx)))
                    .child(render_content(self, cx))
                    .child(status_bar::render_status_bar(self, cx))))
            .when(self.modal.is_some(), |d| {
                let m = self.modal.clone().unwrap();
                d.child(modal::render_modal(self, cx, &m))
            })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Contenido
// ═══════════════════════════════════════════════════════════════════════════

fn render_content(state: &AppState, cx: &mut Context<AppState>) -> AnyElement {
    use crate::ui::views::*;
        use crate::ui::views::widgets::status_dot;
    match state.nav {
        Nav::Terminal => terminal::render_terminal_area(state, cx).into_any_element(),
        Nav::Hosts => hosts::render_hosts_view(state, cx).into_any_element(),
        Nav::Keychain => keychain::render_keychain_view(state, cx).into_any_element(),
        Nav::Snippets => snippets::render_snippets_view(state, cx).into_any_element(),
        Nav::PortForwarding => port_forward::render_port_forward_view(state, cx).into_any_element(),
        Nav::KnownHosts => known_hosts::render_known_hosts_view(state).into_any_element(),
        Nav::Logs => logs::render_logs_view(state, cx).into_any_element(),
        Nav::Settings => settings::render_settings_view(state, cx).into_any_element(),
        Nav::Sftp => sftp::render_sftp_view(state, cx).into_any_element(),
        Nav::Termia => render_termia_view(state, cx),
        Nav::Git => git::render_git_view(state, cx).into_any_element(),
    }
}

fn render_termia_view(state: &AppState, cx: &mut Context<AppState>) -> AnyElement {
    let theme = cx.theme().clone();
    v_flex().size_full().bg(theme.background)
        .child(render_chat_view(&state.chat_state, cx))
        .child(render_input_bar(InputMode::Ai, "".to_string(), |_, _, _| {}, cx))
        .into_any_element()
}

// ═══════════════════════════════════════════════════════════════════════════
// Hosts
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// Keychain
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// Port Forwarding
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// Snippets
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// Known Hosts
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// Logs
// ═══════════════════════════════════════════════════════════════════════════

/// Map a GPUI KeyDownEvent to terminal byte sequence for SSH PTY.

// ═══════════════════════════════════════════════════════════════════════════
// Settings
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// SFTP Dual-Pane File Browser
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// Modales
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// Componentes reutilizables
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// Status bar
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// Entry point
// ═══════════════════════════════════════════════════════════════════════════

/// Keep a Tokio runtime alive for SSH connections (russh requires tokio::net::TcpStream).
static TOKIO_RT: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime")
});

pub fn run(data_dir: PathBuf) {
    // Warm up the Tokio runtime (kept alive via LazyLock)
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
