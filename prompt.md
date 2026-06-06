# ShellMounter — Prompt de arquitectura y construcción

## 🎯 Objetivo

Crear un clon open-source de [Termius](https://termius.com) — un cliente SSH/SFTP de escritorio, nativo, rápido, multi-plataforma (macOS, Windows, Linux). La app debe ser **100% Rust**, binario único, sin dependencias de runtime externas, sin Electron, sin Go.

**Principio rector:** usar las mejores bibliotecas Rust existentes para cada capa. No reinventar SSH, emulación de terminal, ni crypto.

---

## 🧱 Arquitectura

```
┌─────────────────────────────────────────────────────────┐
│  ShellMounter — binario único Rust (~15MB)              │
│                                                         │
│  ┌──────────────────────────────────────────────────┐   │
│  │  UI (GPUI)                                        │   │
│  │  ┌─────────┐ ┌──────────┐ ┌────────┐ ┌────────┐  │   │
│  │  │Host tree│ │Terminal  │ │ SFTP   │ │Snippets│  │   │
│  │  │(grupos, │ │(tabs,    │ │ browser│ │        │  │   │
│  │  │ tags)   │ │ splits)  │ │        │ │        │  │   │
│  │  └────┬────┘ └────┬─────┘ └───┬────┘ └────────┘  │   │
│  └───────┼───────────┼───────────┼──────────────────┘   │
│          │           │           │                      │
│  ┌───────▼───────────▼───────────▼──────────────────┐   │
│  │  Session Manager                                  │   │
│  │  • Pool de conexiones SSH (russh)                 │   │
│  │  • PTY lifecycle por pestaña                      │   │
│  │  • Reconnect automático                           │   │
│  │  • Keep-alive                                     │   │
│  │  • Port forwarding (local/remote)                 │   │
│  └────────────────────────┬─────────────────────────┘   │
│                           │                             │
│  ┌────────────────────────▼─────────────────────────┐   │
│  │  Terminal Engine (alacritty_terminal)              │   │
│  │  • ANSI/VT parser                                  │   │
│  │  • Grid rendering (scrollback ∞)                   │   │
│  │  • Selección de texto + clipboard                  │   │
│  │  • Hipervínculos (OSC 8)                           │   │
│  └──────────────────────────────────────────────────┘   │
│                                                         │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────────┐    │
│  │ Vault    │ │ DB       │ │ Platform             │    │
│  │ (AES)    │ │ (SQLite) │ │ (keychain, menubar)  │    │
│  │          │ │          │ │                      │    │
│  │ Llaves   │ │ Hosts    │ │ macOS: Keychain      │    │
│  │ Passwords│ │ Grupos   │ │ Linux: Secret Svc    │    │
│  │ Certs    │ │ Tags     │ │ Win: Credential Mgr  │    │
│  └──────────┘ └──────────┘ └──────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

---

## 🔧 Stack tecnológico (todo Rust, cero Go)

| Crate | Versión | Rol | Por qué |
|-------|---------|-----|---------|
| **gpui** | git | UI desktop GPU-accelerated | Motor de Zed editor |
| **russh** | 0.46 | Cliente SSH async (tokio) | Puro Rust. Usado por Warp. PTY interactivo, SFTP, port forwarding |
| **russh-sftp** | 0.46 | SFTP client | Parte del ecosistema russh |
| **alacritty_terminal** | 0.24 | Emulación de terminal ANSI/VT | Motor de Alacritty (64K ⭐) |
| **tokio** | 1.x | Runtime async | Estándar para russh y networking |
| **rusqlite** | 0.31 | SQLite para hosts, grupos, config | Con `bundled` feature (sin dependencia de sistema) |
| **aes-gcm** + **argon2** | 0.10 / 0.5 | Vault cifrado | AES-256-GCM + derivación de clave |
| **keyring** | 2.x | OS keychain (master password) | macOS Keychain, Linux Secret Service, Windows Credential Manager |
| **serde** + **serde_json** | 1.x | Serialización | Configuración, export/import de hosts |
| **dirs** | 5.x | Paths de sistema | `~/.shellmounter/` cross-platform |
| **russh-keys** | 0.46 | Parseo de llaves SSH | ED25519, RSA, ECDSA |
| **uuid** | 1.x | IDs únicos | Hosts, sesiones, snippets |

---

## 📁 Estructura del proyecto

```
shellmounter/
├── Cargo.toml
├── prompt.md                   ← este archivo
├── README.md
├── src/
│   ├── main.rs                 # entrypoint: inicia GPUI, carga DB
│   ├── ssh/
│   │   ├── mod.rs
│   │   ├── session.rs          # Pool de conexiones russh, PTY, reconnecting
│   │   ├── channel.rs          # Canal PTY: stdin/stdout/resize
│   │   └── sftp.rs             # Cliente SFTP (russh-sftp)
│   ├── terminal/
│   │   ├── mod.rs
│   │   └── view.rs             # alacritty_terminal → stream → GPUI render
│   ├── vault/
│   │   ├── mod.rs
│   │   ├── crypto.rs           # AES-256-GCM + argon2
│   │   └── store.rs            # Guardar/leer llaves y passwords cifrados
│   ├── db/
│   │   ├── mod.rs
│   │   ├── hosts.rs            # CRUD hosts, grupos, tags (rusqlite)
│   │   └── migrate.rs          # Migraciones SQL
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── app.rs              # Ventana principal, layout de tabs
│   │   ├── host_tree.rs        # Sidebar izquierdo: árbol de hosts
│   │   ├── terminal_tab.rs     # Tab de terminal: renderiza alacritty
│   │   ├── sftp_panel.rs       # Panel SFTP: grid de archivos remotos
│   │   ├── snippet_panel.rs    # Panel de snippets guardados
│   │   ├── host_editor.rs      # Modal: añadir/editar host
│   │   ├── vault_unlock.rs     # Modal: desbloquear vault con master password
│   │   ├── port_forward.rs     # Panel: port forwarding rules
│   │   └── theme.rs            # Temas de terminal (16-colores, catppuccin, etc.)
│   └── platform/
│       ├── mod.rs
│       ├── macos.rs            # Keychain, menubar icon
│       ├── linux.rs            # Secret Service, systray
│       └── windows.rs          # Credential Manager, systray
└── themes/                     # Archivos .toml de temas de terminal
    ├── catppuccin-mocha.toml
    ├── dracula.toml
    ├── one-dark.toml
    └── solarized-dark.toml
```

---

## 🔄 Flujo de uso

```
1. Usuario abre ShellMounter por primera vez
   │
2. App inicializa:
   │  └─ Crea ~/.shellmounter/ si no existe
   │  └─ Abre SQLite (hosts.db)
   │  └─ Detecta OS keychain para vault
   │
3. Modal "Crear Master Password" (solo primera vez)
   │  └─ argon2(password, salt) → AES-256 key
   │  └─ Guarda salt cifrado en disco
   │  └─ Guarda key en OS keychain (opcional)
   │
4. Usuario añade host "prod-db" desde sidebar
   │  └─ Host: 10.0.1.50:22
   │  └─ Usuario: admin
   │  └─ Auth: llave SSH (~/.ssh/id_ed25519)
   │  └─ La llave se guarda en el vault cifrado
   │
5. Usuario hace doble click en "prod-db"
   │  └─ Session Manager crea conexión russh
   │  └─ Abre PTY interactivo
   │  └─ alacritty_terminal empieza a renderizar
   │  └─ Nueva tab con la shell remota
   │
6. Usuario escribe comandos en la terminal
   │  └─ GPUI captura teclas
   │  └─ → stdin a russh channel
   │  └─ ← stdout de russh → alacritty parser → grid render
   │  └─ Latencia: sub-milisegundo (todo en memoria)
   │
7. Usuario necesita transferir un archivo
   │  └─ Botón "SFTP" en la tab del host
   │  └─ Panel SFTP: grid de archivos remotos (russh-sftp)
   │  └─ Drag & drop desde/hacia el filesystem local
   │
8. Usuario cierra ShellMounter
   │  └─ Todas las sesiones SSH se cierran
   │  └─ Vault se bloquea (keys se borran de memoria)
   │  └─ SQLite guarda estado de hosts y config
```

---

## 🎨 Decisiones de diseño

### ¿Por qué 100% Rust y no Go+Rust como AppMounter?
- AppMounter necesitaba alist (Go) porque OAuth con Google Drive, Dropbox, OneDrive, S3, etc. son protocolos complejos con 30+ implementaciones distintas.
- ShellMounter solo necesita SSH. Rust tiene `russh`, una biblioteca SSH cliente/servidor pura Rust mantenida por Warp y Eugeny (creador de Tabby terminal).
- Sin Go = un solo binario, un solo lenguaje, instalación trivial.

### ¿Por qué russh y no ssh2 (libssh2 bindings)?
- `ssh2` es un binding a libssh2 (C). Requiere instalar `libssh2-dev` en el sistema. Rompe la promesa de "binario único sin dependencias".
- `russh` es 100% Rust. Se compila con `cargo build --release` y listo. Igual que Go compila estático.
- `russh` tiene soporte nativo para Tokio (async), PTY interactivo, SFTP, y port forwarding. Mismas capacidades, cero C.

### ¿Por qué alacritty_terminal?
- Es el motor de terminal de Alacritty, el emulador más rápido del ecosistema (64K ⭐ en GitHub).
- Renderizado GPU (OpenGL/Metal/Vulkan). Scrollback infinito. Selección de texto.
- Soporta OSC 8 (hipervínculos), modo vi, search, hints para URLs.
- La integración con GPUI es directa: GPUI ya es GPU-accelerated. alacritty_terminal produce un grid que GPUI puede texturizar.

### ¿Por qué AES-256-GCM + argon2 para el vault?
- `aes-gcm` es una crate Rust pura, auditada. Sin dependencias C.
- `argon2` es el estándar moderno para derivación de claves (ganador del Password Hashing Competition).
- Alternativa más simple: `secretbox` de `crypto_box` (NaCl/libsodium en Rust). Menos código, misma seguridad.
- Las llaves NUNCA se escriben en disco sin cifrar. Solo existen descifradas en memoria mientras el vault está desbloqueado.

### ¿Por qué rusqlite con `bundled`?
- SQLite compilado desde fuente (feature `bundled`). Cero dependencias de sistema.
- Mismo patrón que Alacritty, Zed, y la mayoría de apps Rust.
- La DB guarda: hosts, grupos, tags, snippets, configuración de UI, temas.

---

## 📋 Fases de implementación

### Fase 1 — Core (terminal funcional)
- [ ] `Cargo.toml` con gpui, russh, alacritty_terminal, tokio, rusqlite, aes-gcm, argon2, keyring
- [ ] `db/hosts.rs` — SQLite schema + CRUD de hosts con rusqlite
- [ ] `vault/crypto.rs` — AES-256-GCM encrypt/decrypt + argon2 key derivation
- [ ] `vault/store.rs` — Guardar/leer llaves SSH cifradas
- [ ] `ssh/session.rs` — Conexión con russh, PTY interactivo, stdin/stdout channels
- [ ] `terminal/view.rs` — Integrar alacritty_terminal con GPUI (grid → textura → ventana)
- [ ] `ui/app.rs` — Ventana principal con layout de tabs
- [ ] `ui/terminal_tab.rs` — Una tab que renderiza alacritty + envía teclas a russh
- [ ] `ui/host_editor.rs` — Modal para añadir host (hostname, puerto, usuario, auth)
- [ ] `ui/vault_unlock.rs` — Modal de master password al iniciar

### Fase 2 — Gestión de hosts
- [ ] `ui/host_tree.rs` — Sidebar con árbol de hosts (grupos plegables, tags, search)
- [ ] Importar hosts desde `~/.ssh/config` (parser de SSH config)
- [ ] Exportar/importar hosts en JSON (backup, compartir con equipo)
- [ ] Puerta de enlace SSH (bastion/jump host) vía ProxyJump de russh

### Fase 3 — SFTP y archivos
- [ ] `ssh/sftp.rs` — Cliente SFTP con russh-sftp
- [ ] `ui/sftp_panel.rs` — Panel con grid de archivos remotos
- [ ] Upload/download con barra de progreso
- [ ] Drag & drop desde/hacia el filesystem local
- [ ] Editor de archivos remotos (abrir en editor local, guardar cambios)

### Fase 4 — Experiencia completa
- [ ] `ui/snippet_panel.rs` — Librería de snippets (comandos frecuentes)
- [ ] `ui/port_forward.rs` — Panel de port forwarding (local/remote/dynamic)
- [ ] Temas de terminal (carga desde `themes/*.toml`, catppuccin, dracula, etc.)
- [ ] Splits de terminal (horizontal/vertical en la misma tab)
- [ ] Búsqueda en scrollback (Ctrl+Shift+F)
- [ ] Historial de comandos por host
- [ ] Notificaciones (comando terminó en host inactivo)

---

## 📚 Referencias

- **russh**: https://github.com/Eugeny/russh (1.7K ⭐, Rust, MIT) — Cliente/servidor SSH puro Rust
- **alacritty_terminal**: https://github.com/alacritty/alacritty (64K ⭐, Rust, Apache-2.0) — Motor de terminal
- **GPUI**: https://www.gpui.rs — Framework UI de Zed editor
- **Zed**: https://github.com/zed-industries/zed — Referencia de integración GPUI + terminal
- **Termius**: https://termius.com — Referencia de funcionalidad (app cerrada)
- **Tabby**: https://github.com/Eugeny/tabby — Otra referencia de cliente SSH (usa Angular + xterm.js)

---

## ⚠️ Notas

- **russh** es mantenido por Eugeny (Tabby terminal) bajo el paraguas de Warp. Es el fork activo de thrussh (abandonado). Está en desarrollo activo con releases frecuentes.
- **alacritty_terminal** no es un crate "oficial" separado — es parte del repo de Alacritty y se publica como `alacritty_terminal` en crates.io. La API es estable (Alacritty mismo la usa).
- **GPUI** está en evolución activa. Pinear una revisión específica en `Cargo.toml` con `rev = "..."` en lugar de versión.
- Para el **vault**, considerar usar `crypto_secretbox` de `crypto_box` como alternativa más simple a AES-256-GCM + argon2. Misma seguridad, menos código. Evaluar durante la implementación.
- El parser de `~/.ssh/config` es un mini-proyecto en sí mismo. La crate `ssh-config` (Rust) puede ayudar, pero es mejor empezar con un parser simple para los casos comunes (Host, HostName, User, Port, IdentityFile, ProxyJump).
