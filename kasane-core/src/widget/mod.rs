//! Declarative widget system: KDL-defined status bar items, annotations, and transforms.

mod backend;
pub mod condition;
pub mod parse;
pub mod template;
pub mod types;
pub mod variables;

#[cfg(test)]
mod tests;

pub use backend::{WidgetBackend, node_error_to_diagnostic};
pub use parse::{WidgetNodeError, parse_widget_nodes, parse_widgets};
pub use types::{FaceOrToken, WidgetFile, WidgetKind};
pub use variables::LineContextResolver;
