//! `kasane widget check` CLI subcommand.

use crate::cli::WidgetSubcommand;
use kasane_core::config::config_path;

pub fn execute(subcmd: WidgetSubcommand) -> Result<(), String> {
    match subcmd {
        WidgetSubcommand::Check { path, watch } => {
            let file_path = match path {
                Some(p) => std::path::PathBuf::from(p),
                None => config_path(),
            };

            check_file(&file_path)?;

            if watch {
                run_watch_loop(&file_path)?;
            }

            Ok(())
        }
    }
}

fn check_file(file_path: &std::path::Path) -> Result<(), String> {
    let source = std::fs::read_to_string(file_path)
        .map_err(|e| format!("cannot read {}: {e}", file_path.display()))?;

    match kasane_core::config::unified::parse_unified(&source) {
        Ok((config, widget_file, errors)) => {
            println!(
                "{}: {} widget(s) parsed",
                file_path.display(),
                widget_file.widgets.len()
            );

            // Check @token references against theme faces.
            // Config keys use `_` notation (e.g. "my_accent"), while @token refs
            // are normalized to `.` notation (e.g. "my.accent"). Normalize both
            // for comparison.
            let theme_keys: Vec<String> = config
                .theme
                .faces
                .keys()
                .map(|s| s.replace('_', "."))
                .collect();
            for widget in &widget_file.widgets {
                for token in collect_face_tokens(widget) {
                    if !theme_keys.contains(&token) {
                        // Also check built-in tokens (status.line, menu.item.normal, etc.)
                        // by loading a default theme.
                        let is_builtin = kasane_core::render::theme::Theme::default_theme()
                            .get(&kasane_core::element::StyleToken::new(&token))
                            .is_some();
                        if !is_builtin {
                            eprintln!(
                                "  warning: {}: @{} references undefined theme token",
                                widget.name,
                                token.replace('.', "_")
                            );
                        }
                    }
                }
            }

            for error in &errors {
                eprintln!("  warning: {}: {}", error.name, error.message);
            }
            if errors.is_empty() {
                println!("  no errors");
            }
            Ok(())
        }
        Err(e) => Err(format!("{}: {e}", file_path.display())),
    }
}

/// Collect theme token names from a widget's face references.
fn collect_face_tokens(widget: &kasane_core::widget::types::WidgetDef) -> Vec<String> {
    use kasane_core::widget::types::{FaceOrToken, WidgetKind, WidgetPatch};
    let mut tokens = Vec::new();

    fn check_fot(fot: &FaceOrToken, tokens: &mut Vec<String>) {
        if let FaceOrToken::Token(token) = fot {
            tokens.push(token.name().to_string());
        }
    }

    match &widget.kind {
        WidgetKind::Contribution(c) => {
            for part in &c.parts {
                for rule in &part.face_rules {
                    check_fot(&rule.face, &mut tokens);
                }
            }
        }
        WidgetKind::Background(b) => {
            check_fot(&b.face, &mut tokens);
        }
        WidgetKind::Transform(t) => match &t.patch {
            WidgetPatch::ModifyFace(rules) | WidgetPatch::WrapContainer(rules) => {
                for rule in rules {
                    check_fot(&rule.face, &mut tokens);
                }
            }
        },
        WidgetKind::Gutter(g) => {
            for branch in &g.branches {
                for rule in &branch.face_rules {
                    check_fot(&rule.face, &mut tokens);
                }
            }
        }
    }

    tokens
}

#[cfg(feature = "wasm-plugins")]
fn run_watch_loop(file_path: &std::path::Path) -> Result<(), String> {
    use notify::Watcher;

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res
            && (event.kind.is_modify() || event.kind.is_create())
        {
            let _ = tx.send(());
        }
    })
    .map_err(|e| format!("failed to create file watcher: {e}"))?;

    let watch_path = file_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    watcher
        .watch(watch_path, notify::RecursiveMode::NonRecursive)
        .map_err(|e| format!("failed to watch {}: {e}", watch_path.display()))?;

    println!("watching {} for changes...", file_path.display());
    while rx.recv().is_ok() {
        // Brief debounce
        std::thread::sleep(std::time::Duration::from_millis(100));
        // Drain any additional events
        while rx.try_recv().is_ok() {}

        println!("---");
        if let Err(e) = check_file(file_path) {
            eprintln!("  error: {e}");
        }
    }
    Ok(())
}

#[cfg(not(feature = "wasm-plugins"))]
fn run_watch_loop(_file_path: &std::path::Path) -> Result<(), String> {
    Err("--watch requires the 'wasm-plugins' feature (for the notify crate)".to_string())
}
