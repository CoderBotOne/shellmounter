//! ShellMounter — Open-source Termius alternative
//!
//! Stack: GPUI (UI) + russh (SSH) + alacritty_terminal (terminal) + AES-256 (vault)
//!
//! Usage:
//!   shellmounter              Launch GUI
//!   shellmounter --version    Print version
//!   shellmounter --help       Show help

mod db;
mod platform;
mod ssh;
mod terminal;
mod ui;
mod update;
mod vault;

use std::path::PathBuf;

fn main() {
    // Parse CLI args before initializing tracing
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "--version" | "-V" => {
                println!("ShellMounter v{}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--help" | "-h" => {
                print_help();
                return;
            }
            _ => {
                eprintln!("Unknown option: {}\n", args[1]);
                print_help();
                std::process::exit(1);
            }
        }
    }

    // Initialize tracing for diagnostics (non-blocking, writes to file)
    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("shellmounter")
        .join("logs");

    std::fs::create_dir_all(&log_dir).ok();

    let file_appender = tracing_appender::rolling::daily(&log_dir, "shellmounter.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("shellmounter=info".parse().unwrap())
                .add_directive("russh=warn".parse().unwrap()),
        )
        .with_target(false)
        .init();

    log::info!(
        "ShellMounter v{} starting — {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );

    // Check for updates via Cloudflare R2 (non-blocking background thread)
    std::thread::spawn(|| {
        if let Err(e) = update::check() {
            log::debug!("Update check: {e}");
        }
    });

    // Initialize data directory
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("shellmounter");

    std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");

    // Launch GPUI application
    ui::app::run(data_dir);

    // Flush logs before exit
    log::logger().flush();
}

fn print_help() {
    println!(
        "ShellMounter v{}

USAGE:
    shellmounter              Launch the graphical interface
    shellmounter --version    Print version and exit
    shellmounter --help       Show this help message

DATA:
    All data is stored in:  {}/shellmounter/
    ├── hosts.db             SSH host configurations
    ├── vault/               Encrypted keys and passwords
    ├── themes/              Terminal color themes
    └── logs/                Application logs

ENVIRONMENT:
    SSH_TEST_HOST            SSH host for integration tests
    SSH_TEST_PORT            SSH port (default: 22)
    SSH_TEST_USER            SSH username (default: root)
    SSH_TEST_KEY             Path to SSH private key
    RUST_LOG                 Log level (trace, debug, info, warn, error)

REPOSITORY:
    https://github.com/CoderBotOne/shellmounter",
        env!("CARGO_PKG_VERSION"),
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            .display()
    );
}
