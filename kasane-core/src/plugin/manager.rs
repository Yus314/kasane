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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedWinnerDelta {
    pub id: PluginId,
    pub old: Option<PluginDescriptor>,
    pub new: Option<PluginDescriptor>,
}

impl AppliedWinnerDelta {
    pub fn is_added(&self) -> bool {
        self.old.is_none() && self.new.is_some()
    }

    pub fn is_removed(&self) -> bool {
        self.old.is_some() && self.new.is_none()
    }

    pub fn is_replaced(&self) -> bool {
        self.old.is_some() && self.new.is_some()
    }

    pub fn needs_ready(&self) -> bool {
        self.new.is_some()
    }
}

#[derive(Default)]
pub struct PluginApplyResult {
    pub bootstrap: BootstrapEffects,
    pub deltas: Vec<AppliedWinnerDelta>,
}

impl PluginApplyResult {
    pub fn active_set_changed(&self) -> bool {
        !self.deltas.is_empty()
    }

    pub fn ready_targets(&self) -> impl Iterator<Item = &PluginId> {
        self.deltas
            .iter()
            .filter(|delta| delta.needs_ready())
            .map(|delta| &delta.id)
    }
}

struct PlannedPluginRemoval {
    id: PluginId,
    old: PluginDescriptor,
}

struct PlannedPluginUpsert {
    id: PluginId,
    old: Option<PluginDescriptor>,
    new: PluginDescriptor,
    factory: Arc<dyn PluginFactory>,
}

struct PluginApplyPlan {
    removals: Vec<PlannedPluginRemoval>,
    upserts: Vec<PlannedPluginUpsert>,
    next_snapshot: ResolvedPluginSnapshot,
}

enum PluginApplyMode<'a> {
    Initial,
    Reload(&'a AppState),
}

struct PendingPluginCommit {
    result: PluginApplyResult,
    next_snapshot: ResolvedPluginSnapshot,
}

impl PendingPluginCommit {
    pub fn result(&self) -> &PluginApplyResult {
        &self.result
    }

    pub fn filter_disabled_plugins(mut self, disabled_plugins: &[PluginId]) -> Self {
        for plugin_id in disabled_plugins {
            self.next_snapshot.winners.remove(plugin_id);

            let Some(index) = self
                .result
                .deltas
                .iter()
                .position(|delta| &delta.id == plugin_id)
            else {
                continue;
            };
            let delta = &mut self.result.deltas[index];
            match (&delta.old, &delta.new) {
                (None, Some(_)) => {
                    self.result.deltas.remove(index);
                }
                (Some(_), Some(_)) => {
                    delta.new = None;
                }
                _ => {}
            }
        }
        self
    }
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

    fn plan_initial(&self) -> Result<PluginApplyPlan> {
        let catalog = self.collect_and_resolve()?;
        let next_snapshot = catalog.snapshot();
        Ok(PluginApplyPlan {
            removals: vec![],
            upserts: catalog
                .winners
                .into_values()
                .map(|winner| PlannedPluginUpsert {
                    id: winner.descriptor.id.clone(),
                    old: None,
                    new: winner.descriptor,
                    factory: winner.factory,
                })
                .collect(),
            next_snapshot,
        })
    }

    fn plan_reload(&self) -> Result<PluginApplyPlan> {
        let catalog = self.collect_and_resolve()?;
        let new_snapshot = catalog.snapshot();
        let old_snapshot = self.previous.clone();
        let mut removals = Vec::new();
        let mut upserts = Vec::new();

        for (plugin_id, old_descriptor) in old_snapshot
            .winners
            .iter()
            .filter(|(id, _)| !new_snapshot.winners.contains_key(*id))
        {
            removals.push(PlannedPluginRemoval {
                id: plugin_id.clone(),
                old: old_descriptor.clone(),
            });
        }

        for (plugin_id, winner) in catalog.winners {
            let changed = old_snapshot.winner(&plugin_id) != Some(&winner.descriptor);
            if !changed {
                continue;
            }

            upserts.push(PlannedPluginUpsert {
                id: plugin_id,
                old: old_snapshot.winner(&winner.descriptor.id).cloned(),
                new: winner.descriptor,
                factory: winner.factory,
            });
        }

        Ok(PluginApplyPlan {
            removals,
            upserts,
            next_snapshot: new_snapshot,
        })
    }

    fn apply_plan(
        &self,
        registry: &mut PluginRegistry,
        plan: PluginApplyPlan,
        mode: PluginApplyMode<'_>,
    ) -> Result<PendingPluginCommit> {
        let mut result = PluginApplyResult::default();

        for removal in plan.removals {
            if registry.unload_plugin(&removal.id) {
                result.deltas.push(AppliedWinnerDelta {
                    id: removal.id,
                    old: Some(removal.old),
                    new: None,
                });
            }
        }

        for upsert in plan.upserts {
            match mode {
                PluginApplyMode::Initial => {
                    registry.register_backend(upsert.factory.create()?);
                }
                PluginApplyMode::Reload(state) => {
                    let batch = registry.reload_plugin_batch(upsert.factory.create()?, state);
                    result.bootstrap.merge(batch.effects);
                }
            }
            result.deltas.push(AppliedWinnerDelta {
                id: upsert.id,
                old: upsert.old,
                new: Some(upsert.new),
            });
        }

        Ok(PendingPluginCommit {
            result,
            next_snapshot: plan.next_snapshot,
        })
    }

    pub fn snapshot(&self) -> &ResolvedPluginSnapshot {
        &self.previous
    }

    fn commit(&mut self, pending: PendingPluginCommit) -> PluginApplyResult {
        self.previous = pending.next_snapshot;
        pending.result
    }

    pub fn initialize<F>(
        &mut self,
        registry: &mut PluginRegistry,
        collect_diagnostics: F,
    ) -> Result<PluginApplyResult>
    where
        F: FnOnce(&PluginApplyResult, &mut PluginRegistry) -> Vec<PluginDiagnostic>,
    {
        let plan = self.plan_initial()?;
        let pending = self.apply_plan(registry, plan, PluginApplyMode::Initial)?;
        let diagnostics = collect_diagnostics(pending.result(), registry);
        Ok(self.commit(pending.apply_diagnostics(diagnostics)))
    }

    pub fn reload<F>(
        &mut self,
        registry: &mut PluginRegistry,
        state: &AppState,
        collect_diagnostics: F,
    ) -> Result<PluginApplyResult>
    where
        F: FnOnce(&PluginApplyResult, &mut PluginRegistry) -> Vec<PluginDiagnostic>,
    {
        let plan = self.plan_reload()?;
        let pending = self.apply_plan(registry, plan, PluginApplyMode::Reload(state))?;
        let diagnostics = collect_diagnostics(pending.result(), registry);
        Ok(self.commit(pending.apply_diagnostics(diagnostics)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{PluginRank, PluginRevision, PluginSource};

    fn host_descriptor(id: &str, revision: &str) -> PluginDescriptor {
        PluginDescriptor {
            id: PluginId(id.to_string()),
            source: PluginSource::Host {
                provider: "test".to_string(),
            },
            revision: PluginRevision(revision.to_string()),
            rank: PluginRank::HOST,
        }
    }

    #[test]
    fn commit_discards_failed_added_plugin() {
        let plugin_id = PluginId("demo".to_string());
        let descriptor = host_descriptor("demo", "r1");
        let pending = PendingPluginCommit {
            result: PluginApplyResult {
                bootstrap: BootstrapEffects::default(),
                deltas: vec![AppliedWinnerDelta {
                    id: plugin_id.clone(),
                    old: None,
                    new: Some(descriptor.clone()),
                }],
            },
            next_snapshot: ResolvedPluginSnapshot {
                winners: BTreeMap::from([(plugin_id.clone(), descriptor)]),
            },
        };

        let mut manager = PluginManager::new(vec![]);
        let result =
            manager.commit(pending.filter_disabled_plugins(std::slice::from_ref(&plugin_id)));

        assert!(result.deltas.is_empty());
        assert!(manager.snapshot().winner(&plugin_id).is_none());
    }

    #[test]
    fn commit_turns_failed_replacement_into_removal() {
        let plugin_id = PluginId("demo".to_string());
        let old_descriptor = host_descriptor("demo", "r1");
        let new_descriptor = host_descriptor("demo", "r2");
        let pending = PendingPluginCommit {
            result: PluginApplyResult {
                bootstrap: BootstrapEffects::default(),
                deltas: vec![AppliedWinnerDelta {
                    id: plugin_id.clone(),
                    old: Some(old_descriptor.clone()),
                    new: Some(new_descriptor.clone()),
                }],
            },
            next_snapshot: ResolvedPluginSnapshot {
                winners: BTreeMap::from([(plugin_id.clone(), new_descriptor)]),
            },
        };

        let mut manager = PluginManager::new(vec![]);
        let result =
            manager.commit(pending.filter_disabled_plugins(std::slice::from_ref(&plugin_id)));

        assert_eq!(result.deltas.len(), 1);
        assert!(result.deltas[0].is_removed());
        assert_eq!(result.deltas[0].old.as_ref(), Some(&old_descriptor));
        assert!(manager.snapshot().winner(&plugin_id).is_none());
    }

    #[test]
    fn commit_boundary_defers_snapshot_update() {
        let descriptor = host_descriptor("demo", "r1");
        let mut manager = PluginManager::new(vec![]);
        let pending = PendingPluginCommit {
            result: PluginApplyResult::default(),
            next_snapshot: ResolvedPluginSnapshot {
                winners: BTreeMap::from([(PluginId("demo".to_string()), descriptor.clone())]),
            },
        };

        assert!(
            manager
                .snapshot()
                .winner(&PluginId("demo".to_string()))
                .is_none()
        );
        let _ = manager.commit(pending);
        assert_eq!(
            manager.snapshot().winner(&PluginId("demo".to_string())),
            Some(&descriptor)
        );
    }
}
