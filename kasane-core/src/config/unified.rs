//! Unified `kasane.kdl` file: split config sections from widget definitions.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::widget::parse::{WidgetNodeError, WidgetParseError, parse_widget_nodes};
use crate::widget::types::WidgetFile;

use super::Config;
use super::kdl_parser::{ConfigError, parse_config_from_nodes};

/// Reserved top-level node names that are config sections (or structural blocks).
/// All other top-level nodes are widget definitions (deprecated flat form).
pub const CONFIG_SECTIONS: &[&str] = &[
    "ui",
    "scroll",
    "log",
    "theme",
    "menu",
    "search",
    "clipboard",
    "mouse",
    "window",
    "font",
    "colors",
    "plugins",
    "effects",
    "settings",
    "widgets",
];

/// Returns `true` if `name` is a reserved config section.
pub fn is_config_section(name: &str) -> bool {
    CONFIG_SECTIONS.contains(&name)
}

/// Suggest a config section name for a typo, using edit distance.
fn suggest_section(name: &str) -> Option<&'static str> {
    use crate::widget::variables::edit_distance;
    let mut best: Option<(&str, usize)> = None;
    for &section in CONFIG_SECTIONS {
        let dist = edit_distance(name, section);
        if dist <= 3 && (best.is_none() || dist < best.unwrap().1) {
            best = Some((section, dist));
        }
    }
    best.map(|(s, _)| s)
}

/// Parse a unified `kasane.kdl` source into config + widgets.
///
/// Stage 1: KDL syntax parsing (failure = entire file rejected).
/// Stage 2: Nodes are split by name: reserved names → config, rest → widgets.
/// Stage 3: Widget definitions come from `widgets { }` children only.
///           Top-level non-config nodes are rejected as an error.
///
/// Include directives inside `widgets {}` are resolved relative to the
/// default config directory (`config_path().parent()`).
pub fn parse_unified(
    source: &str,
) -> Result<(Config, Vec<ConfigError>, WidgetFile, Vec<WidgetNodeError>), UnifiedParseError> {
    let config_dir = super::config_path().parent().map(Path::to_path_buf);
    parse_unified_with_base(source, config_dir.as_deref())
}

/// Parse a unified `kasane.kdl` source with an explicit base directory for
/// resolving `include` glob patterns inside the `widgets {}` block.
///
/// Pass `None` to disable include resolution (e.g. in tests with inline strings).
pub fn parse_unified_with_base(
    source: &str,
    config_dir: Option<&Path>,
) -> Result<(Config, Vec<ConfigError>, WidgetFile, Vec<WidgetNodeError>), UnifiedParseError> {
    let doc: kdl::KdlDocument = source
        .parse()
        .map_err(|e: kdl::KdlError| UnifiedParseError::Syntax(e.to_string()))?;

    // Reject any top-level nodes that are not recognized config sections.
    let unknown_names: Vec<String> = doc
        .nodes()
        .iter()
        .filter(|n| !is_config_section(n.name().value()))
        .map(|n| {
            let name = n.name().value();
            if let Some(suggestion) = suggest_section(name) {
                format!("{name} (did you mean '{suggestion}'?)")
            } else {
                name.to_string()
            }
        })
        .collect();
    if !unknown_names.is_empty() {
        return Err(UnifiedParseError::UnknownTopLevel(unknown_names));
    }

    // Extract children of the `widgets` block as widget definitions,
    // expanding `include` directives.
    let mut widget_owned: Vec<kdl::KdlNode> = Vec::new();
    let mut included_paths: Vec<PathBuf> = Vec::new();

    // Config nodes that are NOT the `widgets` block.
    let mut config_owned: Vec<kdl::KdlNode> = Vec::new();
    for node in doc.nodes() {
        if node.name().value() == "widgets" {
            if let Some(children) = node.children() {
                for child in children.nodes() {
                    if child.name().value() == "include" {
                        if let Some(config_dir) = config_dir {
                            resolve_include(
                                child,
                                config_dir,
                                &mut widget_owned,
                                &mut included_paths,
                            );
                        } else {
                            tracing::warn!(
                                "widget include directive ignored: no config directory available"
                            );
                        }
                    } else {
                        widget_owned.push(child.clone());
                    }
                }
            }
        } else {
            config_owned.push(node.clone());
        }
    }

    let (config, config_errors) = parse_config_from_nodes(&config_owned);
    let (mut widget_file, widget_errors) =
        parse_widget_nodes(&widget_owned).map_err(UnifiedParseError::Widget)?;
    widget_file.included_paths = included_paths;

    Ok((config, config_errors, widget_file, widget_errors))
}

/// Expand `~` at the start of a path to the user's home directory.
fn expand_tilde(pattern: &str) -> String {
    if let Some(rest) = pattern.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return format!("{home}/{rest}");
    }
    pattern.to_string()
}

/// Resolve a single `include "glob_pattern"` node: expand the glob, read each
/// matched file as KDL, and append its nodes to `widget_nodes`.
fn resolve_include(
    node: &kdl::KdlNode,
    config_dir: &Path,
    widget_nodes: &mut Vec<kdl::KdlNode>,
    included_paths: &mut Vec<PathBuf>,
) {
    // Extract the glob pattern from the first positional argument.
    let pattern = match node.entries().first() {
        Some(entry) if entry.name().is_none() => match entry.value() {
            kdl::KdlValue::String(s) => s.to_string(),
            _ => {
                tracing::warn!("widget include: expected a string argument");
                return;
            }
        },
        _ => {
            tracing::warn!("widget include: missing glob pattern argument");
            return;
        }
    };

    let expanded = expand_tilde(&pattern);

    // Make relative patterns relative to the config directory.
    let abs_pattern = if Path::new(&expanded).is_relative() {
        config_dir.join(&expanded).to_string_lossy().to_string()
    } else {
        expanded
    };

    let entries = match glob::glob(&abs_pattern) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("widget include: invalid glob pattern `{abs_pattern}`: {e}");
            return;
        }
    };

    // Circular include detection via canonicalized paths.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    // Add the main config file itself to prevent self-inclusion.
    if let Ok(canonical) = config_dir.join("kasane.kdl").canonicalize() {
        seen.insert(canonical);
    }

    for entry in entries {
        let path = match entry {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("widget include: glob error: {e}");
                continue;
            }
        };

        // Skip directories.
        if !path.is_file() {
            continue;
        }

        // Circular include detection.
        let canonical = match path.canonicalize() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "widget include: cannot canonicalize `{}`: {e}",
                    path.display()
                );
                continue;
            }
        };
        if !seen.insert(canonical.clone()) {
            tracing::warn!(
                "widget include: skipping duplicate/circular `{}`",
                path.display()
            );
            continue;
        }

        // Track for file watcher.
        included_paths.push(canonical);

        // Read and parse the included file.
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("widget include: cannot read `{}`: {e}", path.display());
                continue;
            }
        };

        let doc: kdl::KdlDocument = match source.parse() {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("widget include: invalid KDL in `{}`: {e}", path.display());
                continue;
            }
        };

        widget_nodes.extend(doc.nodes().iter().cloned());
    }
}

#[derive(Debug)]
pub enum UnifiedParseError {
    Syntax(String),
    Widget(WidgetParseError),
    /// Top-level nodes that are not recognized config sections.
    UnknownTopLevel(Vec<String>),
}

impl std::fmt::Display for UnifiedParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax(msg) => write!(f, "KDL syntax error: {msg}"),
            Self::Widget(e) => write!(f, "widget error: {e}"),
            Self::UnknownTopLevel(names) => {
                write!(
                    f,
                    "unknown top-level node(s): {}; widgets must be inside a `widgets {{ }}` block",
                    names.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for UnifiedParseError {}
