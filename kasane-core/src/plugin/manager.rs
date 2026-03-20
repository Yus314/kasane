use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;

use crate::state::AppState;

use super::{
    BootstrapEffects, PluginDescriptor, PluginFactory, PluginId, PluginProvider, PluginRegistry,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedPluginSnapshot {
    winners: BTreeMap<PluginId, PluginDescriptor>,
}

impl ResolvedPluginSnapshot {
    pub fn winner(&self, id: &PluginId) -> Option<&PluginDescriptor> {
        self.winners.get(id)
    }
}

struct ResolvedWinner {
    descriptor: PluginDescriptor,
    factory: Arc<dyn PluginFactory>,
}

struct ResolvedCatalog {
    winners: BTreeMap<PluginId, ResolvedWinner>,
}

impl ResolvedCatalog {
    fn snapshot(&self) -> ResolvedPluginSnapshot {
        ResolvedPluginSnapshot {
            winners: self
                .winners
                .iter()
                .map(|(id, winner)| (id.clone(), winner.descriptor.clone()))
                .collect(),
        }
    }
}

#[derive(Default)]
pub struct PluginApplyResult {
    pub bootstrap: BootstrapEffects,
    pub ready_targets: Vec<PluginId>,
    pub active_set_changed: bool,
    pub winner_changed: Vec<PluginId>,
}

pub struct PluginManager {
    providers: Vec<Box<dyn PluginProvider>>,
    previous: ResolvedPluginSnapshot,
}

impl PluginManager {
    pub fn new(providers: Vec<Box<dyn PluginProvider>>) -> Self {
        Self {
            providers,
            previous: ResolvedPluginSnapshot::default(),
        }
    }

    pub fn register_initial_winners(&mut self, registry: &mut PluginRegistry) -> Result<()> {
        let catalog = self.collect_and_resolve()?;
        let snapshot = catalog.snapshot();
        for winner in catalog.winners.into_values() {
            registry.register_backend(winner.factory.create()?);
        }
        self.previous = snapshot;
        Ok(())
    }

    pub fn reload(
        &mut self,
        registry: &mut PluginRegistry,
        state: &AppState,
        session_ready: bool,
    ) -> Result<PluginApplyResult> {
        let catalog = self.collect_and_resolve()?;
        let new_snapshot = catalog.snapshot();
        let old_snapshot = self.previous.clone();
        let mut result = PluginApplyResult::default();

        for plugin_id in old_snapshot
            .winners
            .keys()
            .filter(|id| !new_snapshot.winners.contains_key(*id))
            .cloned()
            .collect::<Vec<_>>()
        {
            if registry.unload_plugin(&plugin_id) {
                result.active_set_changed = true;
                result.winner_changed.push(plugin_id);
            }
        }

        for (plugin_id, winner) in catalog.winners {
            let changed = old_snapshot.winner(&plugin_id) != Some(&winner.descriptor);
            if !changed {
                continue;
            }

            let batch = registry.reload_plugin_batch(winner.factory.create()?, state);
            result.bootstrap.merge(batch.effects);
            result.active_set_changed = true;
            result.winner_changed.push(plugin_id.clone());
            if session_ready {
                result.ready_targets.push(plugin_id);
            }
        }

        self.previous = new_snapshot;
        Ok(result)
    }

    pub fn snapshot(&self) -> &ResolvedPluginSnapshot {
        &self.previous
    }

    fn collect_and_resolve(&self) -> Result<ResolvedCatalog> {
        let mut winners: BTreeMap<PluginId, ResolvedWinner> = BTreeMap::new();
        for provider in &self.providers {
            for factory in provider.collect()? {
                let descriptor = factory.descriptor().clone();
                match winners.get(&descriptor.id) {
                    Some(existing)
                        if compare_descriptors(&descriptor, &existing.descriptor)
                            != Ordering::Greater => {}
                    _ => {
                        winners.insert(
                            descriptor.id.clone(),
                            ResolvedWinner {
                                descriptor,
                                factory,
                            },
                        );
                    }
                }
            }
        }
        Ok(ResolvedCatalog { winners })
    }
}

fn compare_descriptors(lhs: &PluginDescriptor, rhs: &PluginDescriptor) -> Ordering {
    lhs.rank
        .cmp(&rhs.rank)
        .then_with(|| lhs.source.cmp(&rhs.source))
        .then_with(|| lhs.revision.cmp(&rhs.revision))
}
