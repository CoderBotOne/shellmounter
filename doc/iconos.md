# ShellMounter — Iconos

## Cómo agregar un ícono

1. Bajá el SVG de [Lucide](https://lucide.dev/icons/) (o cualquier set que uses)
2. Copialo a `assets/icons/` con el nombre en **kebab-case** que coincida con `IconName`
3. Compilá — `rust-embed` lo incrusta automático

## Convención de nombres

`IconName` (PascalCase) → nombre de archivo (kebab-case):

```
IconName::HardDrive        → hard-drive.svg
IconName::SquareTerminal   → square-terminal.svg
IconName::WindowClose      → window-close.svg
IconName::WindowMinimize   → window-minimize.svg
IconName::WindowMaximize   → window-maximize.svg
IconName::WindowRestore    → window-restore.svg
```

Sin tocar código, sin PRs externos.

## Íconos actuales

| Ícono | SVG | Uso |
|---|---|---|
| `Close` | `close.svg` | Botón cerrar modales |
| `Globe` | `globe.svg` | Nav: Known Hosts |
| `HardDrive` | `hard-drive.svg` | Nav: Keychain, Vault, SFTP |
| `Inbox` | `inbox.svg` | Nav: Logs |
| `Network` | `network.svg` | Nav: Hosts, Port Fwd |
| `Search` | `search.svg` | Búsqueda |
| `Settings` | `settings.svg` | Nav: Settings (temas) |
| `SquareTerminal` | `square-terminal.svg` | Nav: Snippets, header |
| `WindowClose` | `window-close.svg` | TitleBar: cerrar |
| `WindowMaximize` | `window-maximize.svg` | TitleBar: maximizar |
| `WindowMinimize` | `window-minimize.svg` | TitleBar: minimizar |
| `WindowRestore` | `window-restore.svg` | TitleBar: restaurar |

## Temas

Los themes JSON están en `assets/themes/` (21 familias, 36+ variantes). Se cargan al iniciar vía `src/assets.rs::load_themes()`.

Para agregar un tema nuevo: soltar el JSON en `assets/themes/`, se incrusta y carga automático.

## Cómo funciona

`src/assets.rs` usa `rust_embed::RustEmbed` para incrustar los SVGs en el binario e implementa `gpui::AssetSource`. Cuando GPUI pide un ícono, busca en `assets/icons/`.
