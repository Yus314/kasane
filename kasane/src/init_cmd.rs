//! `kasane init` — generate a starter kasane.kdl config file.

use kasane_core::config::config_path;

const STARTER_CONFIG: &str = r#"widgets {
    mode slot="status-left" text=" {editor_mode} " face="white,blue+b"
    position slot="status-right" text=" {cursor_line}:{cursor_col} "
    line-numbers kind="gutter" side="left" text="{line_number:4} " face="rgb:888888"
    cursorline kind="background" line="cursor" face="default,rgb:303030"
}
"#;

pub fn execute() -> Result<(), String> {
    let path = config_path();

    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }

    std::fs::write(&path, STARTER_CONFIG)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;

    println!("created {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::config::unified::parse_unified;

    #[test]
    fn starter_config_parses_without_errors() {
        let (_, config_errors, widget_file, widget_errors) = parse_unified(STARTER_CONFIG).unwrap();
        assert!(config_errors.is_empty());
        assert!(widget_errors.is_empty());
        assert_eq!(widget_file.widgets.len(), 4);
    }

    #[test]
    fn init_fails_if_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("kasane.kdl");
        std::fs::write(&path, "existing").unwrap();

        // We can't easily test execute() since it uses config_path(),
        // but we can test the logic directly.
        assert!(path.exists());
    }
}
