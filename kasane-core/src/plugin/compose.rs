//! Monoidal plugin composition framework.
//!
//! Kasane's plugin extension points form well-defined monoids during the
//! **collection** phase (gathering plugin outputs). This module formalizes
//! that structure with traits and concrete composition types.
//!
//! # Extension Point Classification
//!
//! | Extension Point       | Monoid? | Commutative? | Type                          |
//! |-----------------------|---------|--------------|-------------------------------|
//! | Contributions (slots) | Yes     | Yes          | `ContributionSet`             |
//! | Overlays              | Yes     | Yes          | `OverlaySet`                  |
//! | Display directives    | Yes     | Yes          | `DirectiveSet` (display mod)  |
//! | Annotation gutter     | Yes     | Yes          | (tested inline, not wrapped)  |
//! | Annotation background | Yes     | Yes          | (tested inline, not wrapped)  |
//! | Menu item transforms  | Yes     | No           | `MenuTransformChain`          |
//! | Key dispatch          | Yes     | No           | `FirstWins<T>`                |
//! | Cursor style override | Yes     | No           | `FirstWins<T>`                |
//! | Transform chain       | Yes     | No           | `TransformChain`              |
//! | `resolve()`           | **No**  | N/A          | (not modeled)                 |
//!
//! The **resolution** phase (`resolve()`) is fundamentally non-compositional
//! and is intentionally not modeled here. Transform chains are modeled as a
//! non-commutative monoid for algebraic composition of chain membership.

use super::{OverlayContribution, PluginId, SourcedContribution};
use crate::display::DirectiveSet;

/// A monoid: associative binary operation with identity element.
///
/// # Laws
/// - **Left identity**: `compose(empty(), x) == x`
/// - **Right identity**: `compose(x, empty()) == x`
/// - **Associativity**: `compose(compose(a, b), c) == compose(a, compose(b, c))`
pub trait Composable: Sized {
    /// The identity element.
    fn empty() -> Self;
    /// The associative binary operation.
    fn compose(self, other: Self) -> Self;
}

/// Marker trait: `compose(a, b) == compose(b, a)`.
///
/// Types implementing this trait guarantee that plugin evaluation order
/// does not affect the final collected result.
pub trait CommutativeComposable: Composable {}

// ---------------------------------------------------------------------------
// ContributionSet
// ---------------------------------------------------------------------------

/// Monoid over slot contributions: compose = append + sort by `(priority, contributor)`.
///
/// The sorted-merge semantics make this commutative: regardless of the order
/// plugins are evaluated, the final sorted vec is identical.
#[derive(Debug, Clone, PartialEq)]
pub struct ContributionSet {
    items: Vec<SourcedContribution>,
}

impl ContributionSet {
    /// Wrap contributions, normalizing to sorted order.
    pub fn from_vec(mut items: Vec<SourcedContribution>) -> Self {
        items.sort_by_key(|c| (c.contribution.priority, c.contributor.clone()));
        Self { items }
    }

    /// Unwrap into the inner vec (sorted).
    pub fn into_vec(self) -> Vec<SourcedContribution> {
        self.items
    }

    fn sort(&mut self) {
        self.items
            .sort_by_key(|c| (c.contribution.priority, c.contributor.clone()));
    }
}

impl Composable for ContributionSet {
    fn empty() -> Self {
        Self { items: Vec::new() }
    }

    fn compose(mut self, other: Self) -> Self {
        self.items.extend(other.items);
        self.sort();
        self
    }
}

impl CommutativeComposable for ContributionSet {}

// ---------------------------------------------------------------------------
// OverlaySet
// ---------------------------------------------------------------------------

/// Monoid over overlay contributions: compose = append + sort by `(z_index, plugin_id)`.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlaySet {
    items: Vec<OverlayContribution>,
}

impl OverlaySet {
    /// Wrap overlay contributions, normalizing to sorted order.
    pub fn from_vec(mut items: Vec<OverlayContribution>) -> Self {
        items.sort_by_key(|c| (c.z_index, c.plugin_id.clone()));
        Self { items }
    }

    pub fn into_vec(self) -> Vec<OverlayContribution> {
        self.items
    }

    fn sort(&mut self) {
        self.items.sort_by_key(|c| (c.z_index, c.plugin_id.clone()));
    }
}

impl Composable for OverlaySet {
    fn empty() -> Self {
        Self { items: Vec::new() }
    }

    fn compose(mut self, other: Self) -> Self {
        self.items.extend(other.items);
        self.sort();
        self
    }
}

impl CommutativeComposable for OverlaySet {}

// ---------------------------------------------------------------------------
// DirectiveSet (impl for existing type)
// ---------------------------------------------------------------------------

impl Composable for DirectiveSet {
    fn empty() -> Self {
        DirectiveSet::default()
    }

    fn compose(mut self, other: Self) -> Self {
        self.directives.extend(other.directives);
        self.directives.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.plugin_id.cmp(&b.plugin_id))
        });
        self
    }
}

impl CommutativeComposable for DirectiveSet {}

// ---------------------------------------------------------------------------
// MenuTransformChain
// ---------------------------------------------------------------------------

/// Monoid over menu transform plugin ordering: compose = append (non-commutative).
///
/// The order plugins appear in the chain determines how menu items are
/// transformed, so this is not commutative.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuTransformChain {
    plugins: Vec<PluginId>,
}

impl MenuTransformChain {
    pub fn from_vec(plugins: Vec<PluginId>) -> Self {
        Self { plugins }
    }

    pub fn into_vec(self) -> Vec<PluginId> {
        self.plugins
    }
}

impl Composable for MenuTransformChain {
    fn empty() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    fn compose(mut self, other: Self) -> Self {
        self.plugins.extend(other.plugins);
        self
    }
}

// ---------------------------------------------------------------------------
// TransformChain
// ---------------------------------------------------------------------------

/// An entry in the transform chain: a plugin with its priority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformChainEntry {
    pub plugin_id: PluginId,
    pub priority: i16,
}

/// Non-commutative monoid over transform chain entries.
///
/// Compose = append + sort by `(Reverse(priority), plugin_id)`, matching
/// the sort order used in `apply_transform_chain_in_pane`. This is **not**
/// commutative because entries with equal `(priority, plugin_id)` pairs but
/// different plugin identities yield different chains depending on insertion
/// order when stable-sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformChain {
    entries: Vec<TransformChainEntry>,
}

impl TransformChain {
    /// Construct from entries, normalizing to sorted order.
    pub fn from_entries(mut entries: Vec<TransformChainEntry>) -> Self {
        entries.sort_by_key(|e| (std::cmp::Reverse(e.priority), e.plugin_id.clone()));
        Self { entries }
    }

    /// Single-entry chain.
    pub fn single(plugin_id: PluginId, priority: i16) -> Self {
        Self {
            entries: vec![TransformChainEntry {
                plugin_id,
                priority,
            }],
        }
    }

    /// Borrow the sorted entries.
    pub fn entries(&self) -> &[TransformChainEntry] {
        &self.entries
    }

    /// Consume into the inner vec (sorted).
    pub fn into_entries(self) -> Vec<TransformChainEntry> {
        self.entries
    }

    fn sort(&mut self) {
        self.entries
            .sort_by_key(|e| (std::cmp::Reverse(e.priority), e.plugin_id.clone()));
    }
}

impl Composable for TransformChain {
    fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn compose(mut self, other: Self) -> Self {
        self.entries.extend(other.entries);
        self.sort();
        self
    }
}

// TransformChain is intentionally NOT CommutativeComposable.

// ---------------------------------------------------------------------------
// FirstWins<T>
// ---------------------------------------------------------------------------

/// Monoid where the first non-empty value wins: compose = `self.or(other)`.
///
/// Models key dispatch and cursor style override: the first plugin to
/// claim the event/style takes precedence.
#[derive(Debug, Clone, PartialEq)]
pub struct FirstWins<T> {
    value: Option<T>,
}

impl<T> FirstWins<T> {
    pub fn some(value: T) -> Self {
        Self { value: Some(value) }
    }

    pub fn none() -> Self {
        Self { value: None }
    }

    pub fn into_option(self) -> Option<T> {
        self.value
    }
}

impl<T: Clone> Composable for FirstWins<T> {
    fn empty() -> Self {
        Self { value: None }
    }

    fn compose(self, other: Self) -> Self {
        Self {
            value: self.value.or(other.value),
        }
    }
}
