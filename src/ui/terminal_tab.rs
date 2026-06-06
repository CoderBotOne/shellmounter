//! Terminal tab component — bridges SSH session + alacritty_terminal + GPUI.
//!
//! Each tab holds an SSH session and renders its terminal output.
//! Keyboard input is captured and sent to the remote PTY.

use gpui::*;
use crate::ssh::session::SshSession;
use crate::terminal::view::{TerminalView, TerminalSize};

/// A terminal tab bound to an SSH session.
pub struct TerminalTab {
    pub host_id: String,
    pub host_label: String,
    pub terminal: TerminalView,
    pub session: Option<SshSession>,
    pub connected: bool,
}

impl TerminalTab {
    pub fn new(host_id: String, host_label: String, session: SshSession) -> Self {
        Self {
            host_id,
            host_label,
            terminal: TerminalView::new(TerminalSize::new(120, 40)),
            session: Some(session),
            connected: true,
        }
    }

    /// Feed data from SSH to the terminal.
    pub fn feed_ssh_data(&mut self, data: &[u8]) {
        self.terminal.write(data);
    }

    /// Handle keyboard input and send to SSH.
    pub fn handle_key(&mut self, text: &str) {
        let data = self.terminal.handle_input(text);
        // TODO: send data to SSH session
        // if let Some(ref mut session) = self.session {
        //     tokio::spawn(async move { session.send(&data).await });
        // }
    }
}

impl Render for TerminalTab {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x0a0e14))
            .child(
                self.terminal.render(_window, _cx)
            )
    }
}
