use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    h_flex, v_flex, scroll::ScrollableElement as _, ActiveTheme, Icon, Sizable, IconName,
    resizable::{h_resizable, resizable_panel},
};
use gpui::FontWeight;
use crate::ui::app::{AppState, SftpState, Modal, Nav};
use crate::db::hosts::Host;
use super::widgets::avatar_color;


pub fn render_sftp_view(state: &AppState, cx: &mut Context<AppState>) -> impl IntoElement {
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
            .child(h_flex().gap_1().items_center().px_2().py_1().rounded(cx.theme().radius)
                .id("toggle-hidden")
                .bg(cx.theme().secondary).text_xs().text_color(cx.theme().muted_foreground).cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.sftp.show_hidden = !this.sftp.show_hidden;
                    this.load_local_files();
                    cx.notify();
                }))
                .child(if sftp.show_hidden { "Hide hidden" } else { "Show hidden" })))
        // Dual pane — resizable
        .child(div().flex_1().min_h_0()
            .child(h_resizable("sftp-split")
                // Left pane — Local
                .child(resizable_panel()
                    .child(v_flex().size_full()
                        .child(h_flex().h_8().px_2().gap_1().items_center()
                            .border_b_1().border_color(cx.theme().border).bg(cx.theme().secondary)
                            .child(Icon::new(IconName::Folder).small().text_color(cx.theme().muted_foreground))
                            .child(div().text_xs().font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().muted_foreground).child("Local"))
                            .child(div().flex_1())
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).truncate()
                                .child(local_path_display.clone())))
                        .child(div().flex_1().min_h_0().overflow_hidden()
                            .child(v_flex().gap_0().overflow_y_scrollbar().h_full()
                                .child(render_file_row("..", "", true, cx, |this, cx| {
                                    let parent = std::path::Path::new(&this.sftp.local_path)
                                        .parent()
                                        .map(|p| p.to_string_lossy().to_string())
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
                // Right pane — Remote
                .child(resizable_panel()
                    .child(v_flex().size_full()
                        .child(h_flex().h_8().px_2().gap_1().items_center()
                            .border_b_1().border_color(cx.theme().border).bg(cx.theme().secondary)
                            .child(Icon::new(IconName::Globe).small().text_color(cx.theme().muted_foreground))
                            .child(div().text_xs().font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().muted_foreground).child("Remote"))
                            .child(div().flex_1())
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).truncate()
                                .child(if remote_connected {
                                    remote_path_display.clone()
                                } else {
                                    selected.clone().unwrap_or_else(|| "Select a host".into())
                                })))
                        .child(div().flex_1().min_h_0().overflow_hidden()
                            .child(v_flex().gap_0().overflow_y_scrollbar().h_full()
                                .when(remote_loading, |d| d.child(
                                    div().p_4().text_xs().text_color(cx.theme().muted_foreground)
                                        .child("Conectando...")
                                ))
                                .when(remote_connected, |d| {
                                    let mut children: Vec<AnyElement> = vec![
                                        render_file_row("..", "", true, cx, move |this, cx| {
                                            if let Some(ref sftp) = this.sftp.sftp_session.clone() {
                                                let sftp = sftp.clone();
                                                let current = this.sftp.remote_path.clone();
                                                let parent = std::path::Path::new(&current)
                                                    .parent()
                                                    .map(|p| p.to_string_lossy().to_string())
                                                    .unwrap_or_else(|| "/".into());
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
                                })))))
            ))
}

pub fn render_host_item_sftp(host: &Host, selected: Option<String>, cx: &mut Context<AppState>) -> impl IntoElement {
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

pub fn render_host_item(host: &Host, selected: Option<String>, cx: &mut Context<AppState>) -> impl IntoElement {
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

fn file_icon(name: &str, is_dir: bool) -> (IconName, Hsla) {
    if is_dir {
        return (IconName::FolderClosed, hsla(0.60, 0.72, 0.62, 1.0));
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let color = match ext.as_str() {
        "js" | "mjs" | "cjs"              => hsla(0.147, 0.95, 0.55, 1.0), // yellow
        "ts" | "mts" | "cts" | "d.ts"     => hsla(0.583, 0.71, 0.52, 1.0), // blue
        "jsx"                              => hsla(0.55,  0.80, 0.62, 1.0), // cyan
        "tsx"                              => hsla(0.55,  0.68, 0.55, 1.0), // teal
        "html" | "htm"                     => hsla(0.07,  0.78, 0.58, 1.0), // orange
        "css" | "scss" | "sass" | "less"   => hsla(0.64,  0.72, 0.60, 1.0), // periwinkle
        "json" | "jsonc" | "json5"         => hsla(0.33,  0.58, 0.55, 1.0), // green
        "rs"                               => hsla(0.06,  0.78, 0.52, 1.0), // rust
        "toml"                             => hsla(0.06,  0.55, 0.60, 1.0), // amber
        "py" | "pyi" | "pyc"               => hsla(0.58,  0.55, 0.52, 1.0), // python blue
        "go"                               => hsla(0.55,  0.70, 0.58, 1.0), // go cyan
        "rb"                               => hsla(0.0,   0.72, 0.55, 1.0), // ruby
        "php"                              => hsla(0.72,  0.48, 0.58, 1.0), // php purple
        "java" | "kt" | "kts"              => hsla(0.08,  0.70, 0.55, 1.0), // java orange
        "c" | "h"                          => hsla(0.64,  0.58, 0.58, 1.0), // c blue
        "cpp" | "cc" | "cxx" | "hpp"       => hsla(0.64,  0.48, 0.62, 1.0), // c++ blue
        "sh" | "bash" | "zsh" | "fish"     => hsla(0.35,  0.60, 0.52, 1.0), // shell green
        "sql" | "db" | "sqlite" | "sqlite3"=> hsla(0.09,  0.60, 0.58, 1.0), // amber
        "md" | "mdx" | "rst"               => hsla(0.0,   0.0,  0.68, 1.0), // gray
        "xml" | "xsl" | "xsd" | "svg"      => hsla(0.07,  0.62, 0.58, 1.0), // orange-ish
        "yaml" | "yml"                     => hsla(0.58,  0.42, 0.62, 1.0), // lavender
        "env" | "ini" | "cfg" | "conf"     => hsla(0.58,  0.30, 0.60, 1.0), // muted blue
        "lock"                             => hsla(0.0,   0.0,  0.50, 1.0), // dark gray
        "png" | "jpg" | "jpeg" | "gif"
        | "webp" | "ico" | "bmp" | "tiff"  => hsla(0.42,  0.58, 0.55, 1.0), // green
        "zip" | "tar" | "gz" | "bz2"
        | "xz" | "7z" | "rar" | "zst"      => hsla(0.10,  0.72, 0.58, 1.0), // amber
        "pdf"                              => hsla(0.0,   0.70, 0.55, 1.0), // red
        "mp4" | "mkv" | "avi" | "mov"
        | "webm"                           => hsla(0.80,  0.55, 0.58, 1.0), // purple
        "mp3" | "wav" | "flac" | "ogg"     => hsla(0.83,  0.48, 0.60, 1.0), // violet
        _                                  => hsla(0.0,   0.0,  0.58, 1.0), // default gray
    };
    (IconName::File, color)
}

pub fn render_file_row(
    name: &str, size: &str, is_dir: bool,
    cx: &mut Context<AppState>,
    on_click: impl Fn(&mut AppState, &mut Context<AppState>) + 'static,
) -> impl IntoElement {
    let name_owned = name.to_string();
    let size_owned = size.to_string();
    let (icon, color) = file_icon(name, is_dir);
    h_flex().id(ElementId::Name(format!("file-{}", name_owned).into()))
        .px_2().py_1().gap_2().items_center().cursor_pointer()
        .hover(|d| d.bg(cx.theme().secondary))
        .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
        .child(Icon::new(icon).small().text_color(color))
        .child(div().flex_1().min_w_0().text_sm().child(name_owned.clone()))
        .child(div().w(px(70.)).text_xs().text_color(cx.theme().muted_foreground).child(size_owned.clone()))
}
