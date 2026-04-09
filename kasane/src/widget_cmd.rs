//! `kasane widget check` CLI subcommand.

use crate::cli::WidgetSubcommand;
use kasane_core::config::config_path;

pub fn execute(subcmd: WidgetSubcommand) -> Result<(), String> {
    match subcmd {
        WidgetSubcommand::Check {
            path,
            watch,
            verbose,
        } => {
            let file_path = match path {
                Some(p) => std::path::PathBuf::from(p),
                None => config_path(),
            };

            check_file(&file_path, verbose)?;

            if watch {
                run_watch_loop(&file_path, verbose)?;
            }

            Ok(())
        }
        WidgetSubcommand::Variables => {
            print_variables();
            Ok(())
        }
        WidgetSubcommand::Slots => {
            print_slots();
            Ok(())
        }
    }
}

fn print_variables() {
    use kasane_core::widget::variables::{VariableRegistry, VariableScope};

    let registry = VariableRegistry::new();

    println!("Global variables (available in all widgets):");
    println!();
    for def in registry.iter().filter(|d| d.scope == VariableScope::Global) {
        let type_hint = variable_type_hint(def.name);
        println!("  {:<20} {type_hint}", def.name);
    }

    println!();
    println!("Per-line variables (gutter widgets only):");
    println!();
    for def in registry
        .iter()
        .filter(|d| d.scope == VariableScope::PerLine)
    {
        let type_hint = variable_type_hint(def.name);
        println!("  {:<20} {type_hint}", def.name);
    }

    println!();
    println!("Dynamic namespaces:");
    println!();
    let namespaces: &[(&str, &str)] = &[
        ("opt.<name>", "Kakoune ui_option value (set via kakrc)"),
        ("plugin.<name>", "Plugin-exposed variable"),
    ];
    for (name, desc) in namespaces {
        println!("  {name:<20} {desc}");
    }
}

fn variable_type_hint(name: &str) -> &'static str {
    match name {
        "cursor_line" | "cursor_col" | "cursor_count" | "line_count" | "cols" | "rows"
        | "session_count" | "line_number" | "relative_line" => "number",
        "editor_mode" | "status_style" | "cursor_mode" | "active_session" | "filetype"
        | "bufname" => "string",
        "is_focused" | "has_menu" | "has_info" | "is_prompt" | "is_dark" | "is_cursor_line" => {
            "bool"
        }
        _ => "",
    }
}

fn print_slots() {
    let slots: &[(&str, &str)] = &[
        ("status-left", "Left side of the status bar"),
        ("status-right", "Right side of the status bar"),
        ("buffer-left", "Left of the buffer area"),
        ("buffer-right", "Right of the buffer area"),
        ("above-buffer", "Above the buffer"),
        ("below-buffer", "Below the buffer"),
        ("above-status", "Between buffer and status bar"),
    ];

    let targets: &[(&str, &str)] = &[
        ("status", "Status bar"),
        ("buffer", "Buffer area"),
        ("menu", "Completion menu"),
        ("menu-prompt", "Prompt-mode menu"),
        ("menu-inline", "Inline completion menu"),
        ("menu-search", "Search menu"),
        ("info", "Info popup"),
        ("info-prompt", "Prompt-mode info popup"),
        ("info-modal", "Modal info popup"),
    ];

    println!("Contribution slots:");
    println!();
    for (name, desc) in slots {
        println!("  {name:<20} {desc}");
    }

    println!();
    println!("Transform targets:");
    println!();
    for (name, desc) in targets {
        println!("  {name:<20} {desc}");
    }
    println!();
    println!("  status-bar is an alias for status.");
}

fn check_file(file_path: &std::path::Path, verbose: bool) -> Result<(), String> {
    let source = std::fs::read_to_string(file_path)
        .map_err(|e| format!("cannot read {}: {e}", file_path.display()))?;

    match kasane_core::config::unified::parse_unified(&source) {
        Ok((config, config_errors, widget_file, errors)) => {
            for err in &config_errors {
                eprintln!("  config warning: {err}");
            }
            println!(
                "{}: {} widget(s) parsed",
                file_path.display(),
                widget_file.widgets.len()
            );

            if verbose {
                for widget in &widget_file.widgets {
                    print_widget_details(widget);
                }
            }

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

/// Print detailed information about a single widget.
fn print_widget_details(widget: &kasane_core::widget::types::WidgetDef) {
    use kasane_core::widget::types::{WidgetKind, WidgetPatch};

    for effect in &widget.effects {
        let kind_desc = match &effect.kind {
            WidgetKind::Contribution(c) => format!("contribution → {}", c.slot.0),
            WidgetKind::Background(b) => {
                let line = match &b.line_expr {
                    kasane_core::widget::types::LineExpr::CursorLine => "cursor",
                    kasane_core::widget::types::LineExpr::Selection => "selection",
                };
                format!("background (line={line})")
            }
            WidgetKind::Transform(t) => {
                let patch = match &t.patch {
                    WidgetPatch::ModifyFace(_) => "modify-face",
                    WidgetPatch::WrapContainer(_) => "wrap",
                };
                format!("transform → {} ({patch})", t.target.as_str())
            }
            WidgetKind::Gutter(g) => {
                let side = match g.side {
                    kasane_core::plugin::GutterSide::Left => "left",
                    kasane_core::plugin::GutterSide::Right => "right",
                };
                format!("gutter ({side})")
            }
            WidgetKind::Inline(i) => {
                let pat = match &i.pattern {
                    kasane_core::widget::types::InlinePattern::Substring(s) => {
                        format!("\"{s}\"")
                    }
                    kasane_core::widget::types::InlinePattern::Regex(r) => {
                        format!("/{}/", r.as_str())
                    }
                };
                format!("inline (pattern={pat})")
            }
            WidgetKind::VirtualText(_) => "virtual-text".to_string(),
        };

        // Collect variables from templates
        let vars = collect_template_variables(&effect.kind);
        let vars_str = if vars.is_empty() {
            String::new()
        } else {
            format!("  vars: {}", vars.join(", "))
        };

        println!("  {}: {kind_desc}{vars_str}", widget.name);
    }
}

/// Collect template variable names referenced by a widget kind.
fn collect_template_variables(kind: &kasane_core::widget::types::WidgetKind) -> Vec<&str> {
    use kasane_core::widget::types::WidgetKind;
    let mut vars = Vec::new();
    match kind {
        WidgetKind::Contribution(c) => {
            for part in &c.parts {
                vars.extend(part.template.referenced_variables());
            }
        }
        WidgetKind::Gutter(g) => {
            for branch in &g.branches {
                vars.extend(branch.template.referenced_variables());
            }
        }
        WidgetKind::VirtualText(vt) => {
            vars.extend(vt.template.referenced_variables());
        }
        WidgetKind::Background(_) | WidgetKind::Transform(_) | WidgetKind::Inline(_) => {}
    }
    vars.sort();
    vars.dedup();
    vars
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

    fn collect_kind_tokens(kind: &WidgetKind, tokens: &mut Vec<String>) {
        match kind {
            WidgetKind::Contribution(c) => {
                for part in &c.parts {
                    for rule in &part.face_rules {
                        check_fot(&rule.face, tokens);
                    }
                }
            }
            WidgetKind::Background(b) => {
                check_fot(&b.face, tokens);
            }
            WidgetKind::Transform(t) => match &t.patch {
                WidgetPatch::ModifyFace(rules) | WidgetPatch::WrapContainer(rules) => {
                    for rule in rules {
                        check_fot(&rule.face, tokens);
                    }
                }
            },
            WidgetKind::Gutter(g) => {
                for branch in &g.branches {
                    for rule in &branch.face_rules {
                        check_fot(&rule.face, tokens);
                    }
                }
            }
            WidgetKind::Inline(i) => {
                check_fot(&i.face, tokens);
            }
            WidgetKind::VirtualText(vt) => {
                for rule in &vt.face_rules {
                    check_fot(&rule.face, tokens);
                }
            }
        }
    }

    for effect in &widget.effects {
        collect_kind_tokens(&effect.kind, &mut tokens);
    }

    tokens
}

#[cfg(feature = "wasm-plugins")]
fn run_watch_loop(file_path: &std::path::Path, verbose: bool) -> Result<(), String> {
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
        if let Err(e) = check_file(file_path, verbose) {
            eprintln!("  error: {e}");
        }
    }
    Ok(())
}

#[cfg(not(feature = "wasm-plugins"))]
fn run_watch_loop(_file_path: &std::path::Path, _verbose: bool) -> Result<(), String> {
    Err("--watch requires the 'wasm-plugins' feature (for the notify crate)".to_string())
}
