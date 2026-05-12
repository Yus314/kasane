//! Extension point identifiers.
//!
//! The full extension-point dispatch infrastructure (composition rules,
//! handler tables, runtime evaluation) was retired per ADR-045 once the
//! in-tree producer count reached zero. The [`ExtensionPointId`] type
//! survives because the plugin manifest schema still parses
//! `handlers.extensions_defined` / `handlers.extensions_consumed`
//! metadata into this shape, and the WIT `evaluate-extension` export
//! survives until the next major ABI bump batches the wire-level
//! removal.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// Stable identifier for a plugin-defined extension point.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExtensionPointId(pub CompactString);

impl ExtensionPointId {
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_point_id_equality() {
        let a = ExtensionPointId::new("foo.bar");
        let b = ExtensionPointId::new("foo.bar");
        let c = ExtensionPointId::new("foo.baz");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
