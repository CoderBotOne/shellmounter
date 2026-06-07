# ShellMounter

Cliente SSH/SFTP nativo de código abierto — rápido, multiplataforma, alternativa a Termius.

Construido con Rust, [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) y [gpui-component](https://github.com/longbridge/gpui-component).

## Características

- Gestión de hosts SSH con grupos y etiquetas
- Vault cifrado para credenciales (AES-GCM + Argon2)
- Port forwarding local, remoto y dinámico
- Panel SFTP para transferencia de archivos
- Snippets de comandos reutilizables
- Importación de `~/.ssh/config`
- Terminal integrado (Alacritty terminal emulator)
- UI nativa GPU-acelerada (GPUI)

## Requisitos

- Rust 1.95+ (ver `rust-toolchain.toml`)
- En Linux: libfontconfig, libxkbcommon, libwayland / libx11

## Compilar y ejecutar

```bash
# Solo la UI (modo gráfico)
cargo run --features gui

# Solo la lógica de SSH/terminal (sin UI, para pruebas)
cargo run

# Release
cargo build --release --features gui
```

## Estructura

```
src/
  db/          — Base de datos SQLite (hosts, grupos, etiquetas)
  ssh/         — Sesiones SSH, SFTP, port forwarding, snippets
  terminal/    — Terminal emulator (Alacritty backend)
  vault/       — Vault cifrado para credenciales
  platform/    — Código específico de plataforma
  update/      — Auto-actualización
  ui/          — Interfaz gráfica (GPUI + gpui-component)
    app.rs     — Shell principal: sidebar, titlebar, vista de hosts
```

## Dependencias clave

| Crate | Propósito |
|---|---|
| `gpui` | Framework UI GPU-acelerado (del repo de Zed) |
| `gpui-component` | Componentes UI: Sidebar, TitleBar, Root |
| `russh` | Cliente SSH puro en Rust |
| `russh-sftp` | Protocolo SFTP sobre russh |
| `alacritty_terminal` | Emulador de terminal |
| `rusqlite` | Base de datos local (bundled) |
| `aes-gcm` + `argon2` | Vault cifrado |

## Notas de desarrollo

**gpui** se pina al rev `b077f41a` (mismo que usa gpui-component) para evitar
que haya dos versiones incompatibles del crate en el grafo de dependencias.

**gpui-component** se usa desde ruta local (`/home/jaff/proyectos/gpui-component`)
con el mismo rev de gpui fijado en su workspace.

En Linux, la UI usa Client-Side Decorations (CSD) — la ventana dibuja su propio
titlebar con los botones de cierre/minimizar/maximizar. No uses `WAYLAND_DEBUG=1`
al ejecutar, genera output masivo en stderr.

## Licencia

MIT
