# ShellMounter — Guía para Claude

## Comandos esenciales

```bash
# Verificar que compila (NO hagas cargo build tú — el usuario lo hace)
cargo check --features gui

# Tests
cargo test

# Lint
cargo clippy --features gui
```

## Arquitectura

- **`src/ui/app.rs`** — Shell principal de la UI. Todo el layout: TitleBar, Sidebar, vista de hosts, terminal, modales.
- **`src/db/hosts.rs`** — Tipos `Host`, `AuthMethod`, `HostDb`. La base de datos es SQLite local.
- **`src/vault/store.rs`** — Vault cifrado (AES-GCM + Argon2). Guarda las credenciales SSH.
- **`src/ssh/session.rs`** — Sesión SSH usando `russh`. Async con Tokio.
- **`src/terminal/view.rs`** — Terminal emulator. Backend: `alacritty_terminal`. Parser: `VteProcessor`.

## Stack de UI

- **GPUI** (`b077f41a9f26ae5ed7fadfea55a501d34afb25de`): framework GPU-acelerado del repo de Zed.
- **gpui-component** (path local: `/home/jaff/proyectos/gpui-component`): componentes listos — `TitleBar`, `Sidebar`, `SidebarMenu`, `SidebarMenuItem`, `Root`, `IconName`.
- **gpui-component** tiene que estar fijado al mismo rev de gpui que shellmounter, o habrá dos versiones incompatibles del crate (los traits `IntoElement`, `Render`, etc. no se unificarán).

## Reglas de compilación

- El feature `gui` activa toda la UI. Sin él, solo compila la lógica SSH/terminal.
- `cargo check --features gui` es suficiente para verificar; el usuario hace el `cargo build`.
- Rust 1.95+ requerido (ver `rust-toolchain.toml`) — `cold_path` no existe en versiones anteriores.

## Patrones GPUI importantes

### Sidebar
```rust
// Siempre poner .w(px(240.)) — sin esto el sidebar colapsa a ~48px
Sidebar::new("id")
    .w(px(240.))
    .collapsible(SidebarCollapsible::Icon)
    .collapsed(self.collapsed)
    .child(SidebarGroup::new("label").child(
        SidebarMenu::new()
            .child(SidebarMenuItem::new("Hosts")
                .icon(IconName::SquareTerminal)
                .active(self.nav == Nav::Hosts)
                .on_click(cx.listener(|this, _, _, cx| { ... })))
    ))
```

### Opciones de ventana en Linux
```rust
// NO usar TitleBar::title_bar_options() en Linux — tiene appears_transparent:true
// que causa que Wayland trate la ventana como transparente.
#[cfg(not(target_os = "linux"))]
titlebar: Some(TitleBar::title_bar_options()),
#[cfg(target_os = "linux")]
window_background: WindowBackgroundAppearance::Transparent,
#[cfg(target_os = "linux")]
window_decorations: Some(WindowDecorations::Client),
```

### Eventos de click
```rust
// on_click en elementos GPUI necesita .id() previo
div().id("btn-name").on_click(cx.listener(|this, _: &ClickEvent, _w, cx| { ... }))

// SidebarMenuItem y SidebarToggleButton ya tienen su propio id
SidebarMenuItem::new("label").on_click(cx.listener(|this, _, _, cx| { ... }))
```

### AnyElement para ramas if/else
```rust
fn render_content(state: &AppState, cx: &mut Context<AppState>) -> AnyElement {
    if condition {
        something().into_any_element()
    } else {
        other_thing().into_any_element()
    }
}
```

### Root y arranque
```rust
app.run(move |cx: &mut App| {
    gpui_component::init(cx);  // antes de open_window
    cx.open_window(options, move |window, cx| {
        let state = cx.new(move |_cx| AppState::new(data_dir.clone()));
        cx.new(|cx| Root::new(state, window, cx))  // sin .into()
    });
});
```

## Campos de Theme que existen
```rust
cx.theme().background
cx.theme().foreground
cx.theme().border
cx.theme().primary / .primary_foreground / .primary_hover
cx.theme().secondary / .secondary_hover / .secondary_foreground
cx.theme().muted_foreground
cx.theme().sidebar / .sidebar_foreground / .sidebar_border
cx.theme().sidebar_primary / .sidebar_primary_foreground
cx.theme().title_bar / .title_bar_border
cx.theme().accent
cx.theme().radius
```

No existen: `theme.card`, `theme.sidebar_background`, `theme.title_bar_background`.

## Iconos disponibles (gpui-component)
`SquareTerminal`, `HardDrive`, `Network`, `BookOpen`, `Globe`, `Inbox`,
`Search`, `Plus`, `CircleUser`, `Close`, `LayoutDashboard`, `Settings`,
`PanelLeftOpen`, `PanelLeftClose`, `WindowClose`, `WindowMinimize`, `WindowMaximize`

Para ver todos: `ls /home/jaff/proyectos/gpui-component/crates/assets/assets/icons/`
El nombre del SVG se convierte a PascalCase: `hard-drive.svg` → `IconName::HardDrive`.

## Lo que NO hacer
- No hacer `cargo build` — el usuario lo hace.
- No usar `WAYLAND_DEBUG=1` — llena el log con frames del compositor.
- No usar `theme.card` — no existe.
- No llamar `.render(window, cx)` manualmente en un `Sidebar` — ya implementa `IntoElement`.
- No poner `SidebarToggleButton` en el footer del sidebar — va en el TitleBar o en el contenido.
