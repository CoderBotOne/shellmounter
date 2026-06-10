//! Web Preview module — embedded browser via wry.
//!
//! Opens a separate native window with a WebView that can load
//! local dev servers or any URL. Includes devtools support, resize,
//! and auto-detection of running dev servers.
//!
//! Requires: `cargo build --features webview`
//! Linux dep: `libwebkit2gtk-4.1-dev`

use anyhow::Result;
use std::net::TcpStream;
use std::time::Duration;

/// Common dev server ports to auto-detect
const DEV_PORTS: &[u16] = &[3000, 3001, 5173, 5174, 8080, 3002, 4200, 5000, 8000, 9000];

/// Try to detect a running dev server on common ports
pub fn detect_dev_server() -> Option<String> {
    for port in DEV_PORTS {
        let addr = format!("127.0.0.1:{}", port);
        if TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            Duration::from_millis(200),
        )
        .is_ok()
        {
            return Some(format!("http://localhost:{}", port));
        }
    }
    None
}

/// Check if another port is available (for custom URL)
pub fn check_port(port: u16) -> bool {
    TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port).parse().unwrap(),
        Duration::from_millis(200),
    )
    .is_ok()
}

#[cfg(feature = "webview")]
mod imp {
    use super::*;
    use std::sync::{Arc, Mutex};
    use wry::{WebView, WebViewBuilder};

    /// Open a web preview window at the given URL.
    /// Returns a handle that can be used to navigate, resize, or close.
    pub struct WebPreview {
        webview: WebView,
        current_url: Arc<Mutex<String>>,
    }

    impl WebPreview {
        /// Open a new web preview window
        pub fn open(url: &str, title: &str, devtools: bool) -> Result<Self> {
            let current_url = Arc::new(Mutex::new(url.to_string()));

            let webview = WebViewBuilder::new()
                .with_title(title)
                .with_url(url)
                .with_devtools(devtools)
                .with_resizable(true)
                .with_initialization_script(include_str!("../assets/preview_init.js"))
                .build()?;

            Ok(Self { webview, current_url })
        }

        /// Navigate to a new URL
        pub fn navigate(&self, url: &str) {
            *self.current_url.lock().unwrap() = url.to_string();
            let _ = self.webview.load_url(url);
        }

        /// Get current URL
        pub fn current_url(&self) -> String {
            self.current_url.lock().unwrap().clone()
        }
    }
}

/// Stub for when webview feature is disabled
#[cfg(not(feature = "webview"))]
mod imp {
    use super::*;

    pub struct WebPreview;

    impl WebPreview {
        pub fn open(_url: &str, _title: &str, _devtools: bool) -> Result<Self> {
            Err(anyhow::anyhow!("Web preview requires --features webview. Install libwebkit2gtk-4.1-dev and rebuild."))
        }

        pub fn navigate(&self, _url: &str) {}
        pub fn current_url(&self) -> String { String::new() }
    }
}

