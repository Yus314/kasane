//! Static effect footprint analysis (ADR-030 Level 5).
//!
//! An **effect footprint** is the set of [`EffectCategory`] values that a plugin's
//! handlers may produce. The **local footprint** is computed from registration-time
//! transparency flags. The **transitive footprint** additionally accounts for
//! cascade effects: if plugin A sends a `PluginMessage` to plugin B, A's transitive
//! footprint includes B's transitive footprint.
//!
//! The transitive computation is a least fixed point on the finite lattice
//! `(𝒫(EffectCategory), ⊆)`, guaranteed to terminate in `O(|Π|² × |E|)` steps
//! where `|Π|` is the plugin count and `|E|` is the number of effect category bits.

use super::command::EffectCategory;

/// Per-plugin effect footprint computed from registration-time transparency flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectFootprint {
    /// The set of effect categories this plugin's handlers may produce.
    local: EffectCategory,
    /// The transitive footprint including cascade effects.
    /// `None` until [`compute_transitive_footprints`] is called.
    transitive: Option<EffectCategory>,
}

impl EffectFootprint {
    /// Create a footprint for a fully non-transparent plugin (all categories possible).
    pub fn full() -> Self {
        Self {
            local: EffectCategory::all(),
            transitive: None,
        }
    }

    /// Create a footprint for a fully transparent plugin (no Kakoune writing).
    pub fn transparent() -> Self {
        Self {
            local: EffectCategory::all().difference(EffectCategory::KAKOUNE_WRITING),
            transitive: None,
        }
    }

    /// Create a footprint for a plugin with no effect-producing handlers.
    pub fn empty() -> Self {
        Self {
            local: EffectCategory::empty(),
            transitive: None,
        }
    }

    /// The local effect footprint (from registration-time analysis only).
    pub fn local(&self) -> EffectCategory {
        self.local
    }

    /// The transitive effect footprint (including cascade effects).
    /// Returns `None` if transitive computation has not been performed.
    pub fn transitive(&self) -> Option<EffectCategory> {
        self.transitive
    }

    /// The best available footprint: transitive if computed, otherwise local.
    pub fn effective(&self) -> EffectCategory {
        self.transitive.unwrap_or(self.local)
    }

    /// Whether Kakoune-writing is absent from the effective footprint.
    pub fn is_transparent(&self) -> bool {
        !self.effective().contains(EffectCategory::KAKOUNE_WRITING)
    }

    /// Whether this plugin may trigger cascade re-entry.
    pub fn may_cascade(&self) -> bool {
        self.local.intersects(EffectCategory::CASCADE_TRIGGERS)
    }

    /// Set the transitive footprint (called by `compute_transitive_footprints`).
    #[allow(dead_code)] // used by compute_transitive_footprints and tests
    pub(crate) fn set_transitive(&mut self, categories: EffectCategory) {
        self.transitive = Some(categories);
    }
}

/// Compute transitive effect footprints for a set of plugins.
///
/// Conservative approximation: since `PluginMessage` targets are runtime values,
/// any plugin with `PLUGIN_MESSAGE` in its local footprint is assumed to
/// potentially message any other plugin. Similarly, `INPUT_INJECTION` re-enters
/// the full input pipeline (all plugins).
///
/// The algorithm is a least fixed point iteration on the finite lattice
/// `(𝒫(EffectCategory), ⊆)`. Guaranteed to terminate because footprints
/// can only grow and the lattice has bounded height (14 bits).
pub fn compute_transitive_footprints(footprints: &mut [EffectFootprint]) {
    if footprints.is_empty() {
        return;
    }

    // Initialise transitive footprints from local footprints.
    for fp in footprints.iter_mut() {
        fp.transitive = Some(fp.local);
    }

    // Fixed point iteration.
    loop {
        let mut changed = false;

        // Compute the union of all plugins' current transitive footprints.
        // This is the "any plugin" approximation for PluginMessage and InputInjection.
        let global_union: EffectCategory =
            footprints.iter().fold(EffectCategory::empty(), |acc, fp| {
                acc | fp.transitive.unwrap_or(fp.local)
            });

        for fp in footprints.iter_mut() {
            let current = fp.transitive.unwrap_or(fp.local);
            let mut updated = current;

            // If this plugin may send PluginMessage, it transitively inherits
            // all other plugins' footprints (conservative: unknown target).
            if current.contains(EffectCategory::PLUGIN_MESSAGE) {
                updated |= global_union;
            }

            // If this plugin may inject input, it re-enters the full pipeline.
            if current.contains(EffectCategory::INPUT_INJECTION) {
                updated |= global_union;
            }

            // Timer re-entry is self-referential — the plugin's own handlers
            // run again, but that's already captured by its local footprint.
            // No additional propagation needed.

            if updated != current {
                fp.transitive = Some(updated);
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_footprint_is_transparent() {
        let fp = EffectFootprint::empty();
        assert!(fp.is_transparent());
        assert!(!fp.may_cascade());
    }

    #[test]
    fn full_footprint_is_not_transparent() {
        let fp = EffectFootprint::full();
        assert!(!fp.is_transparent());
    }

    #[test]
    fn transparent_footprint_excludes_kakoune_writing() {
        let fp = EffectFootprint::transparent();
        assert!(fp.is_transparent());
        assert!(!fp.local().contains(EffectCategory::KAKOUNE_WRITING));
    }

    #[test]
    fn transitive_overrides_local() {
        let mut fp = EffectFootprint::transparent();
        assert!(fp.is_transparent());
        fp.set_transitive(EffectCategory::KAKOUNE_WRITING);
        assert!(!fp.is_transparent());
    }

    #[test]
    fn compute_transitive_no_cascade() {
        // Two plugins, neither cascades.
        let mut fps = vec![
            EffectFootprint {
                local: EffectCategory::REDRAW,
                transitive: None,
            },
            EffectFootprint {
                local: EffectCategory::CONFIG_MUTATION,
                transitive: None,
            },
        ];
        compute_transitive_footprints(&mut fps);
        assert_eq!(fps[0].transitive(), Some(EffectCategory::REDRAW));
        assert_eq!(fps[1].transitive(), Some(EffectCategory::CONFIG_MUTATION));
    }

    #[test]
    fn compute_transitive_message_propagates_writing() {
        // Plugin 0: sends PluginMessage (transparent locally).
        // Plugin 1: writes to Kakoune.
        // → Plugin 0's transitive footprint should include KAKOUNE_WRITING.
        let mut fps = vec![
            EffectFootprint {
                local: EffectCategory::PLUGIN_MESSAGE | EffectCategory::REDRAW,
                transitive: None,
            },
            EffectFootprint {
                local: EffectCategory::KAKOUNE_WRITING,
                transitive: None,
            },
        ];
        compute_transitive_footprints(&mut fps);
        assert!(
            fps[0]
                .transitive()
                .unwrap()
                .contains(EffectCategory::KAKOUNE_WRITING)
        );
    }

    #[test]
    fn compute_transitive_input_injection_propagates() {
        // Plugin 0: injects input.
        // Plugin 1: writes to Kakoune (input handler).
        // → Plugin 0's transitive footprint should include KAKOUNE_WRITING.
        let mut fps = vec![
            EffectFootprint {
                local: EffectCategory::INPUT_INJECTION,
                transitive: None,
            },
            EffectFootprint {
                local: EffectCategory::KAKOUNE_WRITING,
                transitive: None,
            },
        ];
        compute_transitive_footprints(&mut fps);
        assert!(
            fps[0]
                .transitive()
                .unwrap()
                .contains(EffectCategory::KAKOUNE_WRITING)
        );
    }

    #[test]
    fn compute_transitive_no_plugins_does_not_panic() {
        let mut fps: Vec<EffectFootprint> = vec![];
        compute_transitive_footprints(&mut fps);
    }

    #[test]
    fn compute_transitive_locally_transparent_stays_transparent_without_cascade() {
        let mut fps = vec![EffectFootprint::transparent()];
        compute_transitive_footprints(&mut fps);
        assert!(fps[0].is_transparent());
    }

    #[test]
    fn compute_transitive_chain_propagation() {
        // Plugin 0: sends message → transitively gets everything
        // Plugin 1: sends message → transitively gets everything
        // Plugin 2: writes to Kakoune
        let mut fps = vec![
            EffectFootprint {
                local: EffectCategory::PLUGIN_MESSAGE,
                transitive: None,
            },
            EffectFootprint {
                local: EffectCategory::PLUGIN_MESSAGE,
                transitive: None,
            },
            EffectFootprint {
                local: EffectCategory::KAKOUNE_WRITING,
                transitive: None,
            },
        ];
        compute_transitive_footprints(&mut fps);
        // All three get KAKOUNE_WRITING because 0 and 1 can message anyone
        assert!(
            fps[0]
                .transitive()
                .unwrap()
                .contains(EffectCategory::KAKOUNE_WRITING)
        );
        assert!(
            fps[1]
                .transitive()
                .unwrap()
                .contains(EffectCategory::KAKOUNE_WRITING)
        );
        // Plugin 2 doesn't cascade, so it keeps its local footprint
        assert_eq!(fps[2].transitive(), Some(EffectCategory::KAKOUNE_WRITING));
    }
}
