//! Content annotations — rich Element insertion between buffer lines.
//!
//! Unlike `SpatialDirective` (Fold/Hide) which performs coordinate compression
//! within the DisplayMap, content annotations operate at the Element layer via
//! Flex Column decomposition. This separation allows inserting full Element trees
//! (images, interactive widgets, multi-line blocks) rather than just `Vec<Atom>`.

use crate::element::Element;
use crate::plugin::PluginId;

/// Where a content annotation is anchored relative to buffer lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentAnchor {
    /// Insert content after the given buffer line.
    InsertAfter(usize),
    /// Insert content before the given buffer line.
    InsertBefore(usize),
}

impl ContentAnchor {
    /// The buffer line this anchor references.
    pub fn line(&self) -> usize {
        match self {
            ContentAnchor::InsertAfter(l) | ContentAnchor::InsertBefore(l) => *l,
        }
    }
}

/// A content annotation: a rich Element to be inserted between buffer lines.
#[derive(Debug, Clone)]
pub struct ContentAnnotation {
    /// Where this annotation is anchored.
    pub anchor: ContentAnchor,
    /// The element tree to render at this anchor point.
    pub element: Element,
    /// The plugin that produced this annotation.
    pub plugin_id: PluginId,
    /// Priority for ordering when multiple annotations target the same line.
    /// Lower values are rendered first (closer to the anchor line).
    pub priority: i16,
}

impl ContentAnnotation {
    /// Sort key for deterministic ordering: `(anchor_line, priority, plugin_id)`.
    pub fn sort_key(&self) -> (usize, i16, &PluginId) {
        (self.anchor.line(), self.priority, &self.plugin_id)
    }
}
