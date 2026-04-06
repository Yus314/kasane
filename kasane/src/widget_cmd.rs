//! `kasane widget check` CLI subcommand.

use crate::cli::WidgetSubcommand;
use kasane_core::config::config_path;

pub fn execute(subcmd: WidgetSubcommand) -> Result<(), String> {
    match subcmd {
        WidgetSubcommand::Check { path } => {
            let widget_path = match path {
                Some(p) => std::path::PathBuf::from(p),
                None => config_path()
                    .parent()
                    .expect("config path must have parent")
                    .join("widgets.kdl"),
            };

            let source = std::fs::read_to_string(&widget_path)
                .map_err(|e| format!("cannot read {}: {e}", widget_path.display()))?;

            match kasane_core::widget::parse_widgets(&source) {
                Ok((file, errors)) => {
                    println!(
                        "{}: {} widget(s) parsed",
                        widget_path.display(),
                        file.widgets.len()
                    );
                    for error in &errors {
                        eprintln!("  warning: {}: {}", error.name, error.message);
                    }
                    if errors.is_empty() {
                        println!("  no errors");
                    }
                    Ok(())
                }
                Err(e) => Err(format!("{}: {e}", widget_path.display())),
            }
        }
    }
}
