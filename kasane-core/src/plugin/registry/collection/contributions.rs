//! Contribution collection (slot-keyed contributions from CONTRIBUTOR plugins).

use crate::plugin::algebra::compose::{Composable, ContributionSet};
use crate::plugin::{
    AppView, ContributeContext, Contribution, PluginCapabilities, SlotId, SourcedContribution,
};
use crate::state::DirtyFlags;

use super::super::{ContributionCache, ContributionEntry, PluginView};

impl<'a> PluginView<'a> {
    /// Collect contributions from all plugins for a given region, sorted by priority.
    pub fn collect_contributions(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Vec<Contribution> {
        self.collect_contributions_with_sources(region, state, ctx)
            .into_iter()
            .map(|sc| sc.contribution)
            .collect()
    }

    pub fn collect_contributions_with_sources(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Vec<SourcedContribution> {
        self.slots
            .iter()
            .filter_map(|slot| {
                if !slot.capabilities.contains(PluginCapabilities::CONTRIBUTOR) {
                    return None;
                }
                let result = slot.backend.contribute_to(region, state, ctx);
                result.map(|contribution| SourcedContribution {
                    contributor: slot.backend.id(),
                    contribution,
                })
            })
            .fold(ContributionSet::empty(), |acc, sc| {
                acc.compose(ContributionSet::from_vec(vec![sc]))
            })
            .into_vec()
    }

    /// Collect contributions with per-plugin caching, gated by per-plugin
    /// `slot.state_revision` (u64 mirror of the bridge's `state_hash()`)
    /// plus the current frame's [`DirtyFlags`].
    ///
    /// Reuses the cached contribution when both (a) the plugin's revision
    /// matches the cached `rev_at_collection`, and (b) no dirty bit
    /// intersects the plugin's `view_deps()`.
    pub fn collect_contributions_cached(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        ctx: &ContributeContext,
        cache: &mut ContributionCache,
        dirty: DirtyFlags,
    ) -> Vec<Contribution> {
        self.collect_contributions_with_sources_cached(region, state, ctx, cache, dirty)
            .into_iter()
            .map(|sc| sc.contribution)
            .collect()
    }

    /// Collect contributions with per-plugin caching (with source tracking).
    pub fn collect_contributions_with_sources_cached(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        ctx: &ContributeContext,
        cache: &mut ContributionCache,
        dirty: DirtyFlags,
    ) -> Vec<SourcedContribution> {
        self.slots
            .iter()
            .filter_map(|slot| {
                if !slot.capabilities.contains(PluginCapabilities::CONTRIBUTOR) {
                    return None;
                }

                let plugin_id = slot.backend.id();
                let cache_key = (plugin_id.clone(), region.clone());
                let current_rev = slot.state_revision.unwrap_or(0);
                let view_deps = slot.backend.view_deps();
                let appstate_dirty = dirty.intersects(view_deps);
                let cached_fresh = !appstate_dirty
                    && cache
                        .contributions
                        .get(&cache_key)
                        .is_some_and(|entry| entry.rev_at_collection == current_rev);

                if cached_fresh {
                    cache
                        .contributions
                        .get(&cache_key)
                        .and_then(|entry| entry.sourced.clone())
                } else {
                    let contribution_opt = slot.backend.contribute_to(region, state, ctx);
                    let result = contribution_opt.map(|contribution| SourcedContribution {
                        contributor: plugin_id,
                        contribution,
                    });
                    cache.contributions.insert(
                        cache_key,
                        ContributionEntry {
                            rev_at_collection: current_rev,
                            sourced: result.clone(),
                        },
                    );
                    result
                }
            })
            .fold(ContributionSet::empty(), |acc, sc| {
                acc.compose(ContributionSet::from_vec(vec![sc]))
            })
            .into_vec()
    }
}
