pub mod cli;
pub mod process;

pub use kasane_core;

#[cfg(feature = "wasm-plugins")]
pub use kasane_wasm;

use anyhow::Result;
use kasane_core::config::Config;
use kasane_core::plugin::PluginRegistry;

use cli::UiMode;

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

/// Run kasane with custom plugins registered alongside built-in ones.
pub fn run(register_plugins: impl FnOnce(&mut PluginRegistry) + Send + 'static) {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let action = match cli::parse_cli_args(&args) {
        Ok(action) => action,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    match action {
        cli::CliAction::ShowVersion => {
            println!("kasane {}", env!("CARGO_PKG_VERSION"));
            println!("{}", process::get_kak_version());
        }
        cli::CliAction::ShowHelp => {
            cli::print_help();
        }
        cli::CliAction::DelegateToKak(args) => {
            process::exec_kak(&args);
        }
        cli::CliAction::RunKasane {
            session,
            ui_mode,
            kak_args,
        } => {
            if let Err(e) = run_inner(session, ui_mode, kak_args, register_plugins) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn run_inner(
    session: Option<String>,
    ui_mode: Option<UiMode>,
    kak_args: Vec<String>,
    register_plugins: impl FnOnce(&mut PluginRegistry) + Send + 'static,
) -> Result<()> {
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

    // Wrap user-provided plugin registration to also discover WASM plugins
    let wrapped_register = wrap_with_wasm_discovery(config.plugins.clone(), register_plugins);

    match resolved_ui {
        UiMode::Tui => kasane_tui::run_tui(
            config,
            move || process::spawn_kakoune(&kak_args),
            wrapped_register,
        ),
        #[cfg(feature = "gui")]
        UiMode::Gui => kasane_gui::run_gui(
            config,
            move || process::spawn_kakoune(&kak_args),
            wrapped_register,
        ),
        #[cfg(not(feature = "gui"))]
        UiMode::Gui => {
            let _ = wrapped_register;
            eprintln!("GUI support not compiled. Rebuild with: cargo build --features gui");
            std::process::exit(1);
        }
    }
}

fn wrap_with_wasm_discovery(
    plugins_config: kasane_core::config::PluginsConfig,
    register_plugins: impl FnOnce(&mut PluginRegistry) + Send + 'static,
) -> impl FnOnce(&mut PluginRegistry) + Send + 'static {
    move |registry: &mut PluginRegistry| {
        register_plugins(registry);

        #[cfg(feature = "wasm-plugins")]
        {
            kasane_wasm::discover_and_register(&plugins_config, registry);
        }

        #[cfg(not(feature = "wasm-plugins"))]
        {
            let _ = plugins_config;
        }
    }
}
