//! Declarative widget system: KDL-defined status bar items, annotations, and transforms.

mod backend;
pub mod condition;
pub mod parse;
pub mod single_backend;
pub mod template;
pub mod types;
pub mod variables;

#[cfg(test)]
mod tests;

pub use backend::{WidgetBackend, node_error_to_diagnostic};
pub use parse::{WidgetNodeError, parse_widget_nodes, parse_widgets};
pub use single_backend::{SingleWidgetBackend, hot_reload_widgets, register_all_widgets};
pub use types::{FaceOrToken, Value, WidgetFile, WidgetKind};
pub use variables::LineContextResolver;
