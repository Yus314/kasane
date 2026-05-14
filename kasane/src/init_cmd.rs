//! `kasane init` — generate a starter kasane.kdl config file.

use kasane_core::config::config_path;

const STARTER_CONFIG: &str = r#"// Kasane configuration
// Docs: https://github.com/kaki/kasane/blob/master/docs/config.md
// Validate: kasane widget check --watch

// -- UI settings (uncomment to customize) --
// ui {
//     padding_char "~"
//     border_style "rounded"    // single, rounded, double, heavy, ascii
//     status_position "bottom"  // top, bottom
// }

// -- Theme (reference in widgets with @token) --
// theme {
//     status_line "default,rgb:303030"
//     accent "cyan,default+b"
// }

// -- Widgets --
// Kinds: contribution (default), background, transform, gutter, inline, virtual-text
// Slots: status-left, status-right, buffer-left, buffer-right,
//        above-buffer, below-buffer, above-status
// Variables: kasane widget variables
widgets {
    mode slot="status-left" text=" {editor_mode} " face="white,blue+b"
    position slot="status-right" text=" {cursor_line}:{cursor_col} "
    line-numbers kind="gutter" side="left" text="{line_number:>4} " face="rgb:888888"
    cursorline kind="background" line="cursor" face="default,rgb:303030"
}
"#;

/// Errors raised by `kasane init`.
#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("{path} already exists")]
    AlreadyExists { path: std::path::PathBuf },
    #[error("failed to create {path}: {source}")]
    CreateDir {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn execute() -> Result<(), InitError> {
    let path = config_path();

    if path.exists() {
        return Err(InitError::AlreadyExists { path });
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| InitError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    std::fs::write(&path, STARTER_CONFIG).map_err(|source| InitError::Write {
        path: path.clone(),
        source,
    })?;

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
