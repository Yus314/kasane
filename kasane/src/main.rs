mod cli;
mod process;

use anyhow::Result;
use cli::{CliAction, UiMode, parse_cli_args, print_help};
use kasane_core::config::Config;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let action = match parse_cli_args(&args) {
        Ok(action) => action,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    match action {
        CliAction::ShowVersion => {
            println!("kasane {}", env!("CARGO_PKG_VERSION"));
            println!("{}", process::get_kak_version());
        }
        CliAction::ShowHelp => {
            print_help();
        }
        CliAction::DelegateToKak(args) => {
            process::exec_kak(&args);
        }
        CliAction::RunKasane {
            session,
            ui_mode,
            kak_args,
        } => {
            if let Err(e) = run(session, ui_mode, kak_args) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn run(session: Option<String>, ui_mode: Option<UiMode>, kak_args: Vec<String>) -> Result<()> {
    let config = Config::load();
    let _guard = setup_logging(&config);

    if let Some(ref s) = session {
        tracing::info!("session: {s}");
    }

    let resolved_ui = match ui_mode {
        Some(m) => m,
        None => match config.ui.backend.as_str() {
            "gui" => UiMode::Gui,
            _ => UiMode::Tui,
        },
    };

    match resolved_ui {
        UiMode::Tui => kasane_tui::run_tui(config, move || process::spawn_kakoune(&kak_args)),
        #[cfg(feature = "gui")]
        UiMode::Gui => kasane_gui::run_gui(config, move || process::spawn_kakoune(&kak_args)),
        #[cfg(not(feature = "gui"))]
        UiMode::Gui => {
            eprintln!("GUI support not compiled. Rebuild with: cargo build --features gui");
            std::process::exit(1);
        }
    }
}

fn setup_logging(config: &Config) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = if let Some(ref file) = config.log.file {
        std::path::PathBuf::from(file)
    } else if let Ok(state_home) = std::env::var("XDG_STATE_HOME") {
        std::path::PathBuf::from(state_home).join("kasane")
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("kasane")
    } else {
        return None;
    };

    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "kasane.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = std::env::var("KASANE_LOG").unwrap_or_else(|_| config.log.level.clone());

    let subscriber = tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(env_filter)
        .with_ansi(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber).ok();

    Some(guard)
}
