//! Declarative widget system: KDL-defined status bar items, annotations, and transforms.

mod backend;
pub mod condition;
pub mod parse;
pub mod template;
pub mod types;
pub mod variables;

#[cfg(test)]
mod tests;

pub use backend::WidgetBackend;
pub use parse::parse_widgets;
pub use types::{FaceOrToken, WidgetFile, WidgetKind};
pub use variables::LineContextResolver;
