/// Icon assets for ShellMounter.
/// Bundles only the Lucide SVG icons we actually use (~11 icons vs 100+ in gpui-component-assets).
use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

#[derive(rust_embed::RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
#[include = "themes/**/*.json"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        Self::get(path)
            .map(|f| Some(f.data))
            .ok_or_else(|| anyhow::anyhow!("could not find asset at path \"{}\"", path))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| p.starts_with(path).then(|| p.into()))
            .collect())
    }
}

/// Load all bundled theme JSON files into the ThemeRegistry.
pub fn load_themes(cx: &mut gpui::App) {
    use gpui_component::ThemeRegistry;
    for file in Assets::iter().filter(|p| p.starts_with("themes/")) {
        if let Some(f) = Assets::get(&file) {
            let content = std::str::from_utf8(&f.data).unwrap_or("");
            if let Err(e) = ThemeRegistry::global_mut(cx).load_themes_from_str(content) {
                tracing::warn!("Failed to load theme {}: {}", file, e);
            }
        }
    }
}
