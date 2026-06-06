//! ShellMounter — Open-source Termius alternative
//!
//! Stack: russh (SSH) + alacritty_terminal (terminal) + AES-256 (vault)
//! The GUI (GPUI) is optional — the CLI mode works on any platform.
//!
//! Usage:
//!   shellmounter              Launch GUI (requires display)
//!   shellmounter --version    Print version
//!   shellmounter --help       Show help
//!   shellmounter --cli        CLI mode (no GUI needed)
//!   shellmounter import       Import hosts from ~/.ssh/config

mod db;
mod platform;
mod ssh;
#[cfg(feature = "alacritty_terminal")]
mod terminal;
mod update;
mod vault;

// UI is optional — requires GPUI (not available on all platforms)
#[cfg(feature = "gui")]
mod ui;

use std::path::PathBuf;

fn main() {
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
            "--cli" => {
                run_cli();
                return;
            }
            "import" => {
                import_hosts();
                return;
            }
            _ => {}
        }
    }

    // Try GUI, fall back to CLI
    #[cfg(feature = "gui")]
    {
        init_logging();
        let data_dir = get_data_dir();
        ui::app::run(data_dir);
        return;
    }

    #[cfg(not(feature = "gui"))]
    {
        println!("GUI not available on this platform. Use --cli or --help.");
        println!("Build with: cargo build --features gui");
    }
}

fn run_cli() {
    init_logging();
    let data_dir = get_data_dir();

    let hosts = db::hosts::HostDb::open(&data_dir)
        .and_then(|db| db.list_hosts(None))
        .unwrap_or_default();

    println!("ShellMounter CLI");
    println!("Hosts: {}", hosts.len());
    for host in &hosts {
        println!("  {} — {}@{}:{}", host.label, host.username, host.hostname, host.port);
    }
}

fn import_hosts() {
    init_logging();
    let data_dir = get_data_dir();

    let ssh_config = dirs::home_dir()
        .unwrap_or_default()
        .join(".ssh")
        .join("config");

    if !ssh_config.exists() {
        eprintln!("No ~/.ssh/config found");
        return;
    }

    match ssh::import_config::parse_ssh_config(&ssh_config) {
        Ok(hosts) => {
            println!("Parsed {} hosts from {}", hosts.len(), ssh_config.display());
            let db = db::hosts::HostDb::open(&data_dir).expect("open DB");
            for host in &hosts {
                match db.upsert_host(host) {
                    Ok(()) => println!("  ✓ {}", host.label),
                    Err(e) => eprintln!("  ✗ {}: {}", host.label, e),
                }
            }
        }
        Err(e) => eprintln!("Failed to parse: {}", e),
    }
}

fn init_logging() {
    let log_dir = get_data_dir().join("logs");
    std::fs::create_dir_all(&log_dir).ok();

    let file_appender = tracing_appender::rolling::daily(&log_dir, "shellmounter.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("shellmounter=info".parse().unwrap()),
        )
        .with_target(false)
        .init();

    log::info!("ShellMounter v{} — CLI mode", env!("CARGO_PKG_VERSION"));
}

fn get_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("shellmounter")
}

fn print_help() {
    println!(
        "ShellMounter v{}

USAGE:
    shellmounter              Launch GUI (requires --features gui)
    shellmounter --cli        CLI mode
    shellmounter import       Import hosts from ~/.ssh/config
    shellmounter --version    Print version
    shellmounter --help       Show this help

DATA:
    {}  /shellmounter/
    ├── hosts.db             SSH host configurations
    ├── vault/               Encrypted keys and passwords
    └── logs/                Application logs

BUILD:
    cargo build --release              CLI only (works everywhere)
    cargo build --release --features gui  With GPUI desktop app

REPO:
    https://github.com/CoderBotOne/shellmounter",
        env!("CARGO_PKG_VERSION"),
        dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).display()
    );
}
