#![allow(dead_code)]
use gpui::prelude::*;
use gpui::*;
use gpui_component::{badge::Badge, button::{Button, ButtonVariants as _}, h_flex, v_flex, ActiveTheme};
use gpui_component::scroll::ScrollableElement as _;
use crate::devtools;
use crate::ui::app::AppState;

pub fn render_devtools_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let nvm = devtools::detect_nvm();
    let scripts = devtools::detect_scripts(&state.data_dir);

    v_flex().size_full().bg(theme.background)
        .child(h_flex().px_4().py_2().gap_2().items_center().border_b_1().border_color(theme.border)
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child("DevTools"))
        )
        .child(
            div().flex_1().overflow_y_scrollbar().p_4().child(v_flex().gap_4()
                .child(render_nvm_section(&nvm, &theme))
                .child(render_scripts_section(&scripts, &theme))
            )
        )
        .into_any_element()
}

fn render_nvm_section(state: &devtools::NvmState, theme: &gpui_component::Theme) -> impl IntoElement {
    v_flex().gap_2()
        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child("Node Version Manager"))
        .child(
            v_flex().gap_1().children(if state.available {
                let mut items: Vec<AnyElement> = Vec::new();
                if let Some(ref cur) = state.current {
                    items.push(h_flex().gap_2().child(Badge::new().child("current")).child(div().text_sm().text_color(theme.foreground).child(cur.clone())).into_any_element());
                }
                for v in &state.installed {
                    items.push(h_flex().gap_2()
                        .child(div().text_sm().text_color(theme.muted_foreground).child(v.clone()))
                        .child(Button::new(format!("nvm-{}", v)).ghost().child("Use"))
                        .into_any_element());
                }
                items
            } else {
                vec![div().text_sm().text_color(theme.muted_foreground).child("NVM not detected. Install from https://github.com/nvm-sh/nvm").into_any_element()]
            })
        )
}

fn render_scripts_section(scripts: &[devtools::ScriptDef], theme: &gpui_component::Theme) -> impl IntoElement {
    let pkg = scripts.iter().filter(|s| s.source == devtools::ScriptSource::PackageJson).collect::<Vec<_>>();
    let make = scripts.iter().filter(|s| s.source == devtools::ScriptSource::Makefile).collect::<Vec<_>>();
    let cargo = scripts.iter().filter(|s| s.source == devtools::ScriptSource::CargoToml).collect::<Vec<_>>();

    v_flex().gap_3()
        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child("Script Runner"))
        .child(script_group("package.json", &pkg, theme))
        .child(script_group("Makefile", &make, theme))
        .child(script_group("Cargo.toml", &cargo, theme))
}

fn script_group(title: &str, scripts: &[&devtools::ScriptDef], theme: &gpui_component::Theme) -> AnyElement {
    if scripts.is_empty() { return div().into_any_element(); }
    v_flex().gap_1()
        .child(div().text_xs().text_color(theme.muted_foreground).child(SharedString::from(title)))
        .child(v_flex().gap_0p5().children(scripts.iter().map(|s| {
            h_flex().px_2().py_1().gap_2().items_center().rounded_md().hover(|st| st.bg(theme.primary))
                .child(Badge::new().child("▶"))
                .child(div().text_sm().text_color(theme.foreground).child(s.name.clone()))
                .child(div().flex_1())
                .child(div().text_xs().text_color(theme.muted_foreground).child(s.command.clone()))
                .child(Button::new(format!("run-{}", s.name)).ghost().child("Run"))
                .into_any_element()
        })))
        .into_any_element()
}

// ═══════════════════════════════════════════════════════════════════════════
// Diff viewer
// ═══════════════════════════════════════════════════════════════════════════

pub fn render_diff_view(old_text: &str, new_text: &str, theme: &gpui_component::Theme) -> impl IntoElement {
    let lines = diff_lines(old_text, new_text);
    v_flex().gap_0().font_family("monospace").text_xs().children(
        lines.iter().map(|(kind, content)| {
            let color = match kind {
                DiffKind::Add => hsla(142.0, 0.71, 0.45, 0.15),
                DiffKind::Remove => hsla(0.0, 0.84, 0.60, 0.15),
                DiffKind::Context => hsla(0.0, 0.0, 0.0, 0.0),
            };
            h_flex().bg(color)
                .child(div().w_8().text_color(match kind { DiffKind::Add=>hsla(142.0,0.7,0.4,1.0), DiffKind::Remove=>hsla(0.0,0.8,0.5,1.0), DiffKind::Context=>hsla(0.0,0.0,0.4,1.0) }).child(kind.prefix()))
                .child(div().text_color(theme.foreground).child(content.clone()))
                .into_any_element()
        })
    )
}

#[derive(Debug, Clone, Copy)]
enum DiffKind { Add, Remove, Context }
impl DiffKind {
    fn prefix(&self) -> &'static str { match self { DiffKind::Add=>"+", DiffKind::Remove=>"-", DiffKind::Context=>" " } }
}

fn diff_lines(old: &str, new: &str) -> Vec<(DiffKind, String)> {
    // Simple line-by-line diff
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut result = Vec::new();

    // Very simple: show all old lines as removed, all new as added
    for l in &old_lines { result.push((DiffKind::Remove, l.to_string())); }
    for l in &new_lines { result.push((DiffKind::Add, l.to_string())); }
    result
}

// ═══════════════════════════════════════════════════════════════════════════
// Markdown preview
// ═══════════════════════════════════════════════════════════════════════════

pub fn render_markdown_preview(content: &str, _cx: &mut Context<AppState>) -> impl IntoElement {
    let theme = _cx.theme().clone();
    let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    v_flex().size_full().bg(theme.background).p_4().overflow_y_scrollbar()
        .child(v_flex().gap_1().children(lines.iter().map(|line| {
            if line.starts_with("# ") {
                let t: SharedString = line.trim_start_matches("# ").into();
                div().text_xl().font_weight(FontWeight::BOLD).text_color(theme.foreground).child(t).into_any_element()
            } else if line.starts_with("## ") {
                let t: SharedString = line.trim_start_matches("## ").into();
                div().text_lg().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child(t).into_any_element()
            } else if line.starts_with("### ") {
                let t: SharedString = line.trim_start_matches("### ").into();
                div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child(t).into_any_element()
            } else if line.starts_with("```") {
                let s: SharedString = line.clone().into();
                div().text_xs().text_color(theme.muted_foreground).child(s).into_any_element()
            } else if line.starts_with("- ") || line.starts_with("* ") {
                let t: SharedString = format!("  • {}", &line[2..]).into();
                div().text_sm().text_color(theme.foreground).child(t).into_any_element()
            } else if line.starts_with("> ") {
                let t: SharedString = line[2..].to_string().into();
                div().text_sm().text_color(theme.muted_foreground).border_l_2().border_color(theme.primary).pl_2().child(t).into_any_element()
            } else if line.is_empty() {
                div().h_2().into_any_element()
            } else {
                let t: SharedString = line.clone().into();
                div().text_sm().text_color(theme.foreground).child(t).into_any_element()
            }
        })))
        .into_any_element()
}
