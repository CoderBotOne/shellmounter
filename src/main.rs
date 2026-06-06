//! ShellMounter — Open-source Termius alternative
//!
//! Stack: GPUI (UI) + russh (SSH) + alacritty_terminal (terminal) + AES-256 (vault)
//! Memory: mimalloc allocator, std::mem::ManuallyDrop for FFI, arc-swap for hot-reload



mod ssh;
mod terminal;
mod vault;
mod db;
mod ui;
mod platform;
mod update;

fn main() {
    // Initialize tracing for debug diagnostics
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("shellmounter=info".parse().unwrap())
        )
        .init();

    log::info!("ShellMounter v{} starting", env!("CARGO_PKG_VERSION"));

    // Check for updates via Cloudflare R2 (non-blocking)
    let update_handle = std::thread::spawn(|| {
        if let Err(e) = update::check() {
            log::debug!("Update check skipped: {e}");
        }
    });

    // Initialize DB and vault
    let db_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("shellmounter");

    std::fs::create_dir_all(&db_path).expect("Failed to create data directory");

    // Launch GPUI app
    ui::app::run(db_path);

    // Cleanup
    let _ = update_handle.join();
}
