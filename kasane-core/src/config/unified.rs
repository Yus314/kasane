//! Unified `kasane.kdl` file: split config sections from widget definitions.

use crate::widget::parse::{WidgetNodeError, WidgetParseError, parse_widget_nodes};
use crate::widget::types::WidgetFile;

use super::Config;
use super::kdl_parser::parse_config_from_nodes;

/// Reserved top-level node names that are config sections.
/// All other top-level nodes are widget definitions.
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
];

/// Returns `true` if `name` is a reserved config section.
pub fn is_config_section(name: &str) -> bool {
    CONFIG_SECTIONS.contains(&name)
}

/// Parse a unified `kasane.kdl` source into config + widgets.
///
/// Stage 1: KDL syntax parsing (failure = entire file rejected).
/// Stage 2: Nodes are split by name: reserved names → config, rest → widgets.
pub fn parse_unified(
    source: &str,
) -> Result<(Config, WidgetFile, Vec<WidgetNodeError>), UnifiedParseError> {
    let doc: kdl::KdlDocument = source
        .parse()
        .map_err(|e: kdl::KdlError| UnifiedParseError::Syntax(e.to_string()))?;

    let (config_nodes, widget_nodes): (Vec<&kdl::KdlNode>, Vec<&kdl::KdlNode>) = doc
        .nodes()
        .iter()
        .partition(|n| is_config_section(n.name().value()));

    // Clone nodes into owned vectors for the sub-parsers.
    let config_owned: Vec<kdl::KdlNode> = config_nodes.into_iter().cloned().collect();
    let widget_owned: Vec<kdl::KdlNode> = widget_nodes.into_iter().cloned().collect();

    let config = parse_config_from_nodes(&config_owned);
    let (widget_file, widget_errors) =
        parse_widget_nodes(&widget_owned).map_err(UnifiedParseError::Widget)?;

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
