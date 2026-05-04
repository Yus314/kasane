//! Composable Lenses — toggleable display-directive contributors.
//!
//! A `Lens` is a named source of `DisplayDirective`s, registered on
//! the `LensRegistry` (held on `AppState`) and individually
//! toggleable. The dispatch path queries the registry alongside
//! plugin display handlers; enabled lenses contribute their
//! directives to the same `DirectiveSet` plugins produce, so the
//! existing display algebra (`Then` / `Merge` via
//! `algebra_normalize`) handles composition uniformly.
//!
//! ## Why a separate abstraction from plugins
//!
//! A plugin's `display()` handler can already emit directives, so
//! lenses look redundant at first glance. The differentiator is
//! **toggle granularity**:
//!
//! - **Plugin granularity**: enable/disable an entire plugin's
//!   capability set. Coarse — a plugin that bundles 5 lenses gets
//!   them all or none.
//! - **Lens granularity**: enable/disable individual contributions
//!   without unloading the plugin. A lens has a stable
//!   `(plugin_id, name)` identity the user can address from a UI
//!   or CLI.
//!
//! Lenses are also a natural unit of caching — a future Salsa
//! integration can key on `(file_id, line, lens_stack)` so a single
//! lens toggle invalidates one cache entry per line, not the whole
//! frame. The MVP doesn't implement caching; the lens pipeline runs
//! every frame just like plugin display handlers do today.
//!
//! ## Composition
//!
//! Lens output composes through the same algebra as plugin output:
//! enabled lenses' directives are pushed onto the same
//! `DirectiveSet` and resolved via `bridge::resolve_via_algebra`.
//! Composition is monoidal — order-independent for non-conflicting
//! leaves; conflicts resolve by `(priority, plugin_id, ...)` sort
//! key. A lens can supply a `priority()` to control its position
//! in the conflict ordering.
//!
//! ## Identity scope
//!
//! Lens ids are namespaced by `(PluginId, String)` to prevent
//! collisions between plugins. The same plugin can register
//! multiple lenses; cross-plugin name reuse is fine.

use std::collections::{BTreeMap, BTreeSet};

use crate::display::DisplayDirective;
use crate::plugin::{AppView, PluginId};

/// Stable identity for a lens: `(owner_plugin, name)`.
///
/// `name` should be a kebab-case identifier scoped within the
/// owning plugin (e.g. `trailing-whitespace`,
/// `todo-highlight`). Two lenses with the same `(plugin, name)`
/// pair conflict on registration — the second `register` call
/// replaces the first.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LensId {
    pub plugin: PluginId,
    pub name: String,
}

impl LensId {
    pub fn new(plugin: PluginId, name: impl Into<String>) -> Self {
        Self {
            plugin,
            name: name.into(),
        }
    }
}

/// A lens — a named, toggleable source of `DisplayDirective`s.
///
/// Implementations should be cheap to construct and pure with
/// respect to their `display(view)` output (no hidden mutable
/// state). Heavy state lives on the owning plugin; a lens reads
/// it via the `AppView`.
pub trait Lens: Send + Sync {
    /// Stable identity. Used by the registry, UI / CLI toggle,
    /// and (future) Salsa cache key.
    fn id(&self) -> LensId;

    /// Optional human-readable label for UI display. Defaults to
    /// the lens name (`id().name`).
    fn label(&self) -> String {
        self.id().name
    }

    /// Priority in the conflict-resolution ordering. Higher
    /// priority wins on overlap (matches plugin
    /// `display_directive_priority`). Default: 0.
    fn priority(&self) -> i16 {
        0
    }

    /// Emit display directives for the current frame. Called by
    /// the rendering pipeline when the lens is enabled.
    /// Disabled lenses' `display` is never called.
    fn display(&self, view: &AppView<'_>) -> Vec<DisplayDirective>;
}

/// Process-local registry of `Lens` instances and their enable /
/// disable state.
///
/// Held on `AppState`. Cloning the registry preserves both the
/// enabled set and the registered lenses — lenses are reference-
/// counted (`Arc<dyn Lens>`) so the clone is cheap.
#[derive(Clone, Default)]
pub struct LensRegistry {
    lenses: BTreeMap<LensId, std::sync::Arc<dyn Lens>>,
    enabled: BTreeSet<LensId>,
}

impl std::fmt::Debug for LensRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LensRegistry")
            .field("registered", &self.lenses.len())
            .field("enabled", &self.enabled.len())
            .finish()
    }
}

impl PartialEq for LensRegistry {
    /// Equality compares only the enabled-id sets and the
    /// registered-id sets. Lens trait objects are compared by id
    /// (two lenses with the same `LensId` are treated as equal
    /// — the registry rejects duplicate registrations anyway).
    fn eq(&self, other: &Self) -> bool {
        self.enabled == other.enabled && self.lenses.keys().eq(other.lenses.keys())
    }
}

impl Eq for LensRegistry {}

impl LensRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a lens. Replaces any existing registration with
    /// the same `LensId`. New registrations start **disabled**;
    /// call `enable` to activate.
    pub fn register(&mut self, lens: std::sync::Arc<dyn Lens>) {
        let id = lens.id();
        self.lenses.insert(id, lens);
    }

    /// Unregister a lens. Also removes it from the enabled set
    /// if present. No-op if `id` was never registered.
    pub fn unregister(&mut self, id: &LensId) {
        self.lenses.remove(id);
        self.enabled.remove(id);
    }

    /// Enable a registered lens. No-op if `id` is not registered
    /// (silent rather than panicking — caller can race the
    /// registration / unregistration cycle without crashing).
    pub fn enable(&mut self, id: &LensId) {
        if self.lenses.contains_key(id) {
            self.enabled.insert(id.clone());
        }
    }

    /// Disable a registered lens. No-op if not enabled.
    pub fn disable(&mut self, id: &LensId) {
        self.enabled.remove(id);
    }

    /// Toggle a lens's enabled state. Returns the new state
    /// (`true` = enabled). No-op if `id` is not registered;
    /// returns the unchanged state in that case (always
    /// `false`).
    pub fn toggle(&mut self, id: &LensId) -> bool {
        if !self.lenses.contains_key(id) {
            return false;
        }
        if self.enabled.contains(id) {
            self.enabled.remove(id);
            false
        } else {
            self.enabled.insert(id.clone());
            true
        }
    }

    /// True iff `id` is registered AND enabled.
    pub fn is_enabled(&self, id: &LensId) -> bool {
        self.enabled.contains(id)
    }

    /// True iff `id` is registered (regardless of enabled
    /// state).
    pub fn is_registered(&self, id: &LensId) -> bool {
        self.lenses.contains_key(id)
    }

    /// All registered lens ids, in canonical (sorted) order.
    pub fn registered_ids(&self) -> impl Iterator<Item = &LensId> {
        self.lenses.keys()
    }

    /// All enabled lens ids, in canonical (sorted) order.
    pub fn enabled_ids(&self) -> impl Iterator<Item = &LensId> {
        self.enabled.iter()
    }

    /// Number of registered lenses.
    pub fn len(&self) -> usize {
        self.lenses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lenses.is_empty()
    }

    /// Number of enabled lenses.
    pub fn enabled_count(&self) -> usize {
        self.enabled.len()
    }

    /// Collect display directives from every enabled lens.
    ///
    /// Returns `(directive, priority, owner_plugin_id)` triples
    /// suitable for pushing onto a `DirectiveSet` (the same
    /// shape `collect_tagged_display_directives` builds for
    /// plugin output, so the two streams merge cleanly through
    /// the same algebra).
    ///
    /// Iteration order is stable: enabled lenses traverse in
    /// `LensId` sort order.
    pub fn collect_directives(&self, view: &AppView<'_>) -> Vec<(DisplayDirective, i16, PluginId)> {
        let mut out = Vec::new();
        for id in &self.enabled {
            let Some(lens) = self.lenses.get(id) else {
                continue;
            };
            let priority = lens.priority();
            for d in lens.display(view) {
                out.push((d, priority, id.plugin.clone()));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
