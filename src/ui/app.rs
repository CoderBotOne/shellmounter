//! ShellMounter UI — GPUI application entry point.
//!
//! Uses gpui-component for polished UI components:
//! buttons, tabs, sidebar, tree, context_menu, etc.

use gpui::*;
use std::path::PathBuf;
use std::sync::Arc;

use crate::db::hosts::HostDb;
use crate::vault::store::Vault;

/// Global application state.
pub struct ShellMounter {
    host_db: Arc<HostDb>,
    vault: Arc<parking_lot::Mutex<Vault>>,
    data_dir: PathBuf,
}

impl ShellMounter {
    fn new(data_dir: PathBuf) -> Self {
        let host_db = Arc::new(HostDb::open(&data_dir).expect("Failed to open host database"));
        let vault = Arc::new(parking_lot::Mutex::new(
            Vault::open(&data_dir).expect("Failed to open vault"),
        ));

        Self {
            host_db,
            vault,
            data_dir,
        }
    }
}

impl Render for ShellMounter {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .bg(rgb(0x0a0e14)) // Dark theme background
            .child(
                // Left sidebar: host tree
                div()
                    .w(px(260.))
                    .h_full()
                    .border_r_1()
                    .border_color(rgb(0x1a1f2e))
                    .child(Label::new("Hosts").color(rgb(0x8d91a5))),
            )
            .child(
                // Main content: terminal tabs
                div()
                    .flex_1()
                    .h_full()
                    .flex_col()
                    .child(
                        // Tab bar
                        div()
                            .h(px(36.))
                            .border_b_1()
                            .border_color(rgb(0x1a1f2e))
                            .child(Label::new("No connections").color(rgb(0x8d91a5))),
                    )
                    .child(
                        // Terminal area
                        div()
                            .flex_1()
                            .size_full()
                            .bg(rgb(0x0a0e14))
                            .child(
                                Label::new(
                                    "ShellMounter v0.1.0 — Open a host to start"
                                )
                                .color(rgb(0x4a4f62)),
                            ),
                    ),
            )
    }
}

/// Launch the GPUI application.
pub fn run(data_dir: PathBuf) {
    App::new().run(|cx: &mut AppContext| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Point::new(100., 100.),
                    Size::new(1200., 800.),
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("ShellMounter".into()),
                    appears_transparent: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| {
                cx.new(|_cx| ShellMounter::new(data_dir.clone()))
            },
        )
        .unwrap();
    });
}
