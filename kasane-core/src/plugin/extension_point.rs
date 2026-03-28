//! Plugin-defined extension points.
//!
//! Allows plugins to define custom extension points that other plugins can
//! contribute to, without requiring changes to the framework. Each extension
//! point has a typed input/output and a composition rule for combining
//! multiple contributions.
//!
//! # Example
//!
//! ```ignore
//! // Plugin A defines an extension point:
//! r.define_extension::<(), Vec<StatusItem>>(
//!     ExtensionPointId::new("myplugin.status-items"),
//!     CompositionRule::Merge,
//! );
//!
//! // Plugin B contributes to it:
//! r.on_extension::<(), Vec<StatusItem>>(
//!     ExtensionPointId::new("myplugin.status-items"),
//!     |_state, _input, _app| vec![StatusItem { ... }],
//! );
//! ```

use std::any::Any;

use compact_str::CompactString;

use super::{AppView, PluginId, PluginState};

/// Identifier for a plugin-defined extension point.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExtensionPointId(pub CompactString);

impl ExtensionPointId {
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How to compose multiple contributions to an extension point.
#[derive(Debug, Clone)]
pub enum CompositionRule {
    /// Merge all results into a Vec (order by plugin registration order).
    Merge,
    /// First non-empty result wins (priority order).
    FirstWins,
    /// Chain: each handler receives the previous handler's output as input.
    Chain,
}

// =============================================================================
// Type-erased handler types
// =============================================================================

/// Type-erased extension handler: `fn(&dyn PluginState, &dyn Any, &AppView) -> Box<dyn Any>`.
pub(crate) type ErasedExtensionHandler =
    Box<dyn Fn(&dyn PluginState, &dyn Any, &AppView<'_>) -> Box<dyn Any + Send> + Send + Sync>;

/// Registration entry for defining an extension point.
/// Registration entry for defining an extension point.
///
/// Framework-internal. Plugin authors interact with
/// [`HandlerRegistry::define_extension()`] instead.
#[doc(hidden)]
pub struct ExtensionDefinition {
    pub(crate) id: ExtensionPointId,
    #[allow(dead_code)]
    pub(crate) rule: CompositionRule,
    /// The definer's own handler (optional — the definer may also contribute).
    pub(crate) handler: Option<ErasedExtensionHandler>,
}

/// Registration entry for contributing to an extension point.
pub(crate) struct ExtensionContribution {
    pub(crate) id: ExtensionPointId,
    pub(crate) handler: ErasedExtensionHandler,
}

// =============================================================================
// Runtime evaluation
// =============================================================================

/// Collected extension point results after evaluation.
pub struct ExtensionResults {
    results: std::collections::HashMap<ExtensionPointId, Vec<ExtensionOutput>>,
}

/// Output from an extension point handler.
///
/// Framework-internal. Plugin authors use [`ExtensionResults::get()`] to access typed results.
#[doc(hidden)]
pub struct ExtensionOutput {
    #[allow(dead_code)]
    pub(crate) plugin_id: PluginId,
    pub(crate) value: Box<dyn Any + Send>,
}

impl ExtensionResults {
    pub(crate) fn new() -> Self {
        Self {
            results: std::collections::HashMap::new(),
        }
    }

    /// Get all outputs for an extension point, typed.
    pub fn get<T: 'static>(&self, id: &ExtensionPointId) -> Vec<&T> {
        self.results
            .get(id)
            .map(|outputs| {
                outputs
                    .iter()
                    .filter_map(|o| o.value.downcast_ref::<T>())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn insert(&mut self, id: ExtensionPointId, output: ExtensionOutput) {
        self.results.entry(id).or_default().push(output);
    }
}

impl Default for ExtensionResults {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn extension_results_get_typed() {
        let mut results = ExtensionResults::new();
        let id = ExtensionPointId::new("test.ext");
        results.insert(
            id.clone(),
            ExtensionOutput {
                plugin_id: PluginId("p".to_string()),
                value: Box::new(42u32),
            },
        );
        results.insert(
            id.clone(),
            ExtensionOutput {
                plugin_id: PluginId("q".to_string()),
                value: Box::new(99u32),
            },
        );

        let values = results.get::<u32>(&id);
        assert_eq!(values.len(), 2);
        assert_eq!(*values[0], 42);
        assert_eq!(*values[1], 99);
    }

    #[test]
    fn extension_results_empty_for_unknown() {
        let results = ExtensionResults::new();
        let values = results.get::<u32>(&ExtensionPointId::new("nonexistent"));
        assert!(values.is_empty());
    }

    #[test]
    fn extension_results_type_mismatch_filtered() {
        let mut results = ExtensionResults::new();
        let id = ExtensionPointId::new("test");
        results.insert(
            id.clone(),
            ExtensionOutput {
                plugin_id: PluginId("p".to_string()),
                value: Box::new("string value".to_string()),
            },
        );

        // Request as u32 → filtered out
        let values = results.get::<u32>(&id);
        assert!(values.is_empty());

        // Request as String → found
        let values = results.get::<String>(&id);
        assert_eq!(values.len(), 1);
    }
}
