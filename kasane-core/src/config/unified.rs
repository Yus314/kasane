//! Unified `kasane.kdl` file: split config sections from widget definitions.

use crate::widget::parse::{WidgetNodeError, WidgetParseError, parse_widget_nodes};
use crate::widget::types::WidgetFile;

use super::Config;
use super::kdl_parser::parse_config_from_nodes;

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
    "settings",
    "widgets",
];

/// Returns `true` if `name` is a reserved config section.
pub fn is_config_section(name: &str) -> bool {
    CONFIG_SECTIONS.contains(&name)
}

/// Parse a unified `kasane.kdl` source into config + widgets.
///
/// Stage 1: KDL syntax parsing (failure = entire file rejected).
/// Stage 2: Nodes are split by name: reserved names → config, rest → widgets.
/// Stage 3: Widget definitions come from `widgets { }` children and/or
///           flat top-level non-config nodes (deprecated, emits a warning per node).
pub fn parse_unified(
    source: &str,
) -> Result<(Config, WidgetFile, Vec<WidgetNodeError>), UnifiedParseError> {
    let doc: kdl::KdlDocument = source
        .parse()
        .map_err(|e: kdl::KdlError| UnifiedParseError::Syntax(e.to_string()))?;

    let (config_nodes, flat_widget_nodes): (Vec<&kdl::KdlNode>, Vec<&kdl::KdlNode>) = doc
        .nodes()
        .iter()
        .partition(|n| is_config_section(n.name().value()));

    // Extract children of the `widgets` block as widget definitions.
    let mut widget_owned: Vec<kdl::KdlNode> = Vec::new();
    let mut deprecation_names: Vec<String> = Vec::new();

    // Config nodes that are NOT the `widgets` block.
    let mut config_owned: Vec<kdl::KdlNode> = Vec::new();
    for node in &config_nodes {
        if node.name().value() == "widgets" {
            // Extract children as widget definitions
            if let Some(children) = node.children() {
                widget_owned.extend(children.nodes().iter().cloned());
            }
        } else {
            config_owned.push((*node).clone());
        }
    }

    // Flat top-level non-config nodes are the deprecated form.
    for node in &flat_widget_nodes {
        deprecation_names.push(node.name().value().to_string());
        widget_owned.push((*node).clone());
    }

    let config = parse_config_from_nodes(&config_owned);
    let (widget_file, mut widget_errors) =
        parse_widget_nodes(&widget_owned).map_err(UnifiedParseError::Widget)?;

    // Emit deprecation warnings for flat-form widgets.
    for name in deprecation_names {
        widget_errors.push(WidgetNodeError {
            name: name.clone(),
            message: format!(
                "top-level widget '{name}' is deprecated; move it inside a `widgets {{ }}` block"
            ),
        });
    }

    Ok((config, widget_file, widget_errors))
}

#[derive(Debug)]
pub enum UnifiedParseError {
    Syntax(String),
    Widget(WidgetParseError),
}

impl std::fmt::Display for UnifiedParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax(msg) => write!(f, "KDL syntax error: {msg}"),
            Self::Widget(e) => write!(f, "widget error: {e}"),
        }
    }
}

impl std::error::Error for UnifiedParseError {}
