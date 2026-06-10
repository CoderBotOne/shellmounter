//! Termia — Terminal + IA, built on GPUI.
//!
//! Stack: alacritty_terminal (terminal) + local PTY + OpenAI/Anthropic/Ollama (AI)
//! The GUI (GPUI) requires a display server (X11/Wayland).

mod pty;
mod git;
#[cfg(feature = "alacritty_terminal")]
mod terminal;
mod webview;
mod kanban;
mod devtools;

// AI module — Termia: terminal + IA integration
#[cfg(feature = "gui")]
mod ai;

// UI requires GPUI
#[cfg(feature = "gui")]
mod ui;

#[cfg(feature = "gui")]
mod assets;

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "--version" | "-V" => {
                println!("Termia v{}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--help" | "-h" => {
                print_help();
                return;
            }
            _ => {}
        }
    }

    #[cfg(feature = "gui")]
    {
        init_logging();
        let data_dir = get_data_dir();
        ui::app::run(data_dir);
        return;
    }

    #[cfg(not(feature = "gui"))]
    {
        println!("GUI not available. Build with: cargo build --features gui");
    }
}

fn init_logging() {
    let log_dir = get_data_dir().join("logs");
    std::fs::create_dir_all(&log_dir).ok();

    let file_appender = tracing_appender::rolling::daily(&log_dir, "termia.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("termia=info".parse().unwrap()),
        )
        .with_target(false)
        .init();

    log::info!("Termia v{}", env!("CARGO_PKG_VERSION"));
}

fn get_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("termia")
}

fn print_help() {
    println!(
        "Termia v{}
        
USAGE:
    termia              Launch GUI (requires --features gui)
    termia --version    Print version
    termia --help       Show this help

DATA:
    {}  /termia/
    └── logs/           Application logs

FEATURES:
    Terminal  — Local PTY shell (bash/zsh)
    AI Chat   — OpenAI, Anthropic, Ollama
    Git       — Stage, commit, push, branch tree
    Kanban    — Task board with columns and cards
    DevTools  — NVM selector, script runner, diff viewer

BUILD:
    cargo build --release --features gui    Desktop app

REPO:
    https://github.com/CoderBotOne/termia",
        env!("CARGO_PKG_VERSION"),
        dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).display()
    );
}
