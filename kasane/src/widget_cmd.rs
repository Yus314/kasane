//! `kasane widget check` CLI subcommand.

use crate::cli::WidgetSubcommand;
use kasane_core::config::config_path;

pub fn execute(subcmd: WidgetSubcommand) -> Result<(), String> {
    match subcmd {
        WidgetSubcommand::Check { path } => {
            let file_path = match path {
                Some(p) => std::path::PathBuf::from(p),
                None => config_path(),
            };

            let source = std::fs::read_to_string(&file_path)
                .map_err(|e| format!("cannot read {}: {e}", file_path.display()))?;

            match kasane_core::config::unified::parse_unified(&source) {
                Ok((_config, widget_file, errors)) => {
                    println!(
                        "{}: {} widget(s) parsed",
                        file_path.display(),
                        widget_file.widgets.len()
                    );
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
    }
}
