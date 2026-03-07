mod process;

use anyhow::Result;

use kasane_core::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiMode {
    Tui,
    Gui,
}

fn main() -> Result<()> {
    // Parse CLI arguments
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (session, ui_mode, kak_args) = parse_cli_args(&args);

    // Load config
    let config = Config::load();

    // Setup logging
    let _guard = setup_logging(&config);

    match ui_mode {
        UiMode::Tui => {
            let session_clone = session.clone();
            let kak_args_clone = kak_args.clone();
            kasane_tui::run_tui(config, move || {
                if let Some(ref s) = session_clone {
                    process::connect_kakoune(s, &kak_args_clone)
                } else {
                    process::spawn_kakoune(&kak_args_clone)
                }
            })
        }
        #[cfg(feature = "gui")]
        UiMode::Gui => {
            let session_clone = session.clone();
            let kak_args_clone = kak_args.clone();
            kasane_gui::run_gui(config, move || {
                if let Some(ref s) = session_clone {
                    process::connect_kakoune(s, &kak_args_clone)
                } else {
                    process::spawn_kakoune(&kak_args_clone)
                }
            })
        }
        #[cfg(not(feature = "gui"))]
        UiMode::Gui => {
            eprintln!("GUI support not compiled. Rebuild with: cargo build --features gui");
            std::process::exit(1);
        }
    }
}

fn parse_cli_args(args: &[String]) -> (Option<String>, UiMode, Vec<String>) {
    let mut session = None;
    let mut ui_mode = UiMode::Tui;
    let mut kak_args = Vec::new();
    let mut iter = args.iter().peekable();
    let mut pass_through = false;

    while let Some(arg) = iter.next() {
        if pass_through {
            kak_args.push(arg.clone());
            continue;
        }
        match arg.as_str() {
            "-c" => {
                if let Some(s) = iter.next() {
                    session = Some(s.clone());
                }
            }
            "--ui" => {
                if let Some(mode) = iter.next() {
                    match mode.as_str() {
                        "gui" => ui_mode = UiMode::Gui,
                        "tui" => ui_mode = UiMode::Tui,
                        _ => {
                            eprintln!("unknown --ui mode: {mode}. Use 'tui' or 'gui'.");
                            std::process::exit(1);
                        }
                    }
                }
            }
            "--" => {
                pass_through = true;
            }
            _ => {
                kak_args.push(arg.clone());
            }
        }
    }

    (session, ui_mode, kak_args)
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
