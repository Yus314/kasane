//! Contribution collection (slot-keyed contributions from CONTRIBUTOR plugins).

use crate::plugin::compose::{Composable, ContributionSet};
use crate::plugin::{
    AppView, ContributeContext, Contribution, PluginCapabilities, SlotId, SourcedContribution,
};

use super::super::{ContributionCache, PluginView};

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

    /// Collect contributions with per-plugin caching.
    ///
    /// Only calls `contribute_to()` for plugins whose `needs_recollect` is true.
    /// For non-stale plugins, the cached result from the previous frame is reused.
    pub fn collect_contributions_cached(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        ctx: &ContributeContext,
        cache: &mut ContributionCache,
    ) -> Vec<Contribution> {
        self.collect_contributions_with_sources_cached(region, state, ctx, cache)
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
    ) -> Vec<SourcedContribution> {
        self.slots
            .iter()
            .filter_map(|slot| {
                if !slot.capabilities.contains(PluginCapabilities::CONTRIBUTOR) {
                    return None;
                }

                let plugin_id = slot.backend.id();
                let cache_key = (plugin_id.clone(), region.clone());

                if slot.needs_recollect {
                    let result =
                        slot.backend
                            .contribute_to(region, state, ctx)
                            .map(|contribution| SourcedContribution {
                                contributor: plugin_id,
                                contribution,
                            });
                    cache.contributions.insert(cache_key, result.clone());
                    result
                } else {
                    cache.contributions.get(&cache_key).cloned().flatten()
                }
            })
            .fold(ContributionSet::empty(), |acc, sc| {
                acc.compose(ContributionSet::from_vec(vec![sc]))
            })
            .into_vec()
    }
}
