//! Element key for frame-to-frame element identification.
//!
//! An `ElementKey` identifies a persistent UI element across frames,
//! enabling property animations to survive element tree rebuilds.

/// Stable identifier for a UI element across frames.
///
/// Elements are identified by a combination of a kind discriminator
/// and an optional index for elements that appear in lists (e.g., pane N).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ElementKey {
    kind: ElementKind,
    index: u32,
}

/// Discriminator for well-known UI element types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementKind {
    /// The text cursor.
    Cursor,
    /// A menu/completion overlay.
    Menu,
    /// The info/status bar.
    Info,
    /// A buffer pane (indexed by pane number).
    Pane,
    /// A border element.
    Border,
    /// A plugin-defined custom element.
    Custom(u32),
}

impl ElementKey {
    /// Create a key for a well-known singleton element.
    pub const fn new(kind: ElementKind) -> Self {
        Self { kind, index: 0 }
    }

    /// Create a key for an indexed element (e.g., pane 2).
    pub const fn indexed(kind: ElementKind, index: u32) -> Self {
        Self { kind, index }
    }

    // Pre-defined keys for common elements.
    pub const CURSOR: Self = Self::new(ElementKind::Cursor);
    pub const MENU: Self = Self::new(ElementKind::Menu);
    pub const INFO: Self = Self::new(ElementKind::Info);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singleton_keys_equal() {
        assert_eq!(ElementKey::CURSOR, ElementKey::new(ElementKind::Cursor));
    }

    #[test]
    fn indexed_keys_distinct() {
        let pane0 = ElementKey::indexed(ElementKind::Pane, 0);
        let pane1 = ElementKey::indexed(ElementKind::Pane, 1);
        assert_ne!(pane0, pane1);
    }

    #[test]
    fn custom_keys() {
        let a = ElementKey::new(ElementKind::Custom(42));
        let b = ElementKey::new(ElementKind::Custom(43));
        assert_ne!(a, b);
    }
}
