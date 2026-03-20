use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;

use crate::state::AppState;

use super::diagnostics::{PluginDiagnostic, PluginDiagnosticKind, PluginDiagnosticTarget};
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
    diagnostics: Vec<PluginDiagnostic>,
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
    pub diagnostics: Vec<PluginDiagnostic>,
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
    catalog_diagnostics: Vec<PluginDiagnostic>,
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

    pub fn apply_diagnostics(mut self, mut diagnostics: Vec<PluginDiagnostic>) -> Self {
        for diagnostic in &mut diagnostics {
            let Some(plugin_id) = diagnostic.plugin_id().cloned() else {
                continue;
            };
            self.next_snapshot.winners.remove(&plugin_id);

            let Some(index) = self
                .result
                .deltas
                .iter()
                .position(|delta| delta.id == plugin_id)
            else {
                continue;
            };
            let delta = &mut self.result.deltas[index];
            diagnostic.previous = delta.old.clone();
            diagnostic.attempted = delta.new.clone();
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
        self.result.diagnostics.extend(diagnostics);
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
            catalog_diagnostics: catalog.diagnostics,
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
            catalog_diagnostics: catalog.diagnostics,
        })
    }

    fn apply_plan(
        &self,
        registry: &mut PluginRegistry,
        plan: PluginApplyPlan,
        mode: PluginApplyMode<'_>,
    ) -> Result<PendingPluginCommit> {
        let PluginApplyPlan {
            removals,
            upserts,
            mut next_snapshot,
            ..
        } = plan;
        let mut result = PluginApplyResult::default();

        for removal in removals {
            if registry.unload_plugin(&removal.id) {
                result.deltas.push(AppliedWinnerDelta {
                    id: removal.id,
                    old: Some(removal.old),
                    new: None,
                });
            }
        }

        for upsert in upserts {
            let plugin = match upsert.factory.create() {
                Ok(plugin) => plugin,
                Err(err) => {
                    result.diagnostics.push(PluginDiagnostic {
                        target: PluginDiagnosticTarget::Plugin(upsert.id.clone()),
                        kind: PluginDiagnosticKind::InstantiationFailed,
                        message: err.to_string(),
                        previous: upsert.old.clone(),
                        attempted: Some(upsert.new.clone()),
                    });
                    match &upsert.old {
                        Some(old) => {
                            result.deltas.retain(|delta| delta.id != upsert.id);
                            next_snapshot.winners.insert(upsert.id.clone(), old.clone());
                        }
                        None => {
                            next_snapshot.winners.remove(&upsert.id);
                        }
                    }
                    continue;
                }
            };
            match mode {
                PluginApplyMode::Initial => {
                    registry.register_backend(plugin);
                }
                PluginApplyMode::Reload(state) => {
                    let batch = registry.reload_plugin_batch(plugin, state);
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
            next_snapshot,
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
        let catalog_diagnostics = plan.catalog_diagnostics.clone();
        let pending = self.apply_plan(registry, plan, PluginApplyMode::Initial)?;
        let diagnostics = collect_diagnostics(pending.result(), registry);
        let mut result = self.commit(pending.apply_diagnostics(diagnostics));
        result.diagnostics.extend(catalog_diagnostics);
        Ok(result)
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
        let catalog_diagnostics = plan.catalog_diagnostics.clone();
        let pending = self.apply_plan(registry, plan, PluginApplyMode::Reload(state))?;
        let diagnostics = collect_diagnostics(pending.result(), registry);
        let mut result = self.commit(pending.apply_diagnostics(diagnostics));
        result.diagnostics.extend(catalog_diagnostics);
        Ok(result)
    }

    fn collect_and_resolve(&self) -> Result<ResolvedCatalog> {
        let mut winners: BTreeMap<PluginId, ResolvedWinner> = BTreeMap::new();
        let mut diagnostics = Vec::new();
        for provider in &self.providers {
            let collect = match provider.collect() {
                Ok(collect) => collect,
                Err(err) => {
                    diagnostics.push(PluginDiagnostic::provider_collect_failed(
                        provider.name(),
                        err.to_string(),
                    ));
                    continue;
                }
            };
            diagnostics.extend(collect.diagnostics);
            for factory in collect.factories {
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
        Ok(ResolvedCatalog {
            winners,
            diagnostics,
        })
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
    use crate::plugin::{
        PluginBackend, PluginCollect, PluginRank, PluginRevision, PluginSource, plugin_factory,
    };
    use crate::surface::SurfaceRegistrationError;
    use anyhow::anyhow;
    use std::sync::{Arc, Mutex};

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

    struct DemoPlugin;

    impl PluginBackend for DemoPlugin {
        fn id(&self) -> PluginId {
            PluginId("demo".to_string())
        }
    }

    struct FailingProvider;

    impl PluginProvider for FailingProvider {
        fn name(&self) -> &'static str {
            "failing-provider"
        }

        fn collect(&self) -> Result<PluginCollect> {
            Err(anyhow!("provider collect exploded"))
        }
    }

    #[derive(Clone, Copy)]
    enum FactoryVariant {
        Ok,
        Err,
    }

    #[derive(Clone)]
    struct DemoFactoryProvider {
        variant: Arc<Mutex<FactoryVariant>>,
        revision: Arc<Mutex<&'static str>>,
    }

    impl DemoFactoryProvider {
        fn new(initial: FactoryVariant, revision: &'static str) -> Self {
            Self {
                variant: Arc::new(Mutex::new(initial)),
                revision: Arc::new(Mutex::new(revision)),
            }
        }

        fn set_state(&self, variant: FactoryVariant, revision: &'static str) {
            *self.variant.lock().expect("poisoned factory variant") = variant;
            *self.revision.lock().expect("poisoned factory revision") = revision;
        }
    }

    impl PluginProvider for DemoFactoryProvider {
        fn collect(&self) -> Result<PluginCollect> {
            let variant = *self.variant.lock().expect("poisoned factory variant");
            let revision = *self.revision.lock().expect("poisoned factory revision");
            let descriptor = host_descriptor("demo", revision);
            Ok(PluginCollect {
                factories: vec![plugin_factory(descriptor, move || match variant {
                    FactoryVariant::Ok => Ok(Box::new(DemoPlugin)),
                    FactoryVariant::Err => Err(anyhow!("factory exploded")),
                })],
                diagnostics: vec![],
            })
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
                diagnostics: vec![],
            },
            next_snapshot: ResolvedPluginSnapshot {
                winners: BTreeMap::from([(plugin_id.clone(), descriptor)]),
            },
        };

        let mut manager = PluginManager::new(vec![]);
        let result = manager.commit(pending.apply_diagnostics(vec![
            PluginDiagnostic::surface_registration_failed(
                plugin_id.clone(),
                SurfaceRegistrationError::DuplicateSurfaceKey {
                    surface_key: "duplicate".into(),
                },
            ),
        ]));

        assert!(result.deltas.is_empty());
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].plugin_id(), Some(&plugin_id));
        assert!(result.diagnostics[0].previous.is_none());
        assert_eq!(
            result.diagnostics[0].attempted.as_ref(),
            Some(&host_descriptor("demo", "r1"))
        );
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
                diagnostics: vec![],
            },
            next_snapshot: ResolvedPluginSnapshot {
                winners: BTreeMap::from([(plugin_id.clone(), new_descriptor.clone())]),
            },
        };

        let mut manager = PluginManager::new(vec![]);
        let result = manager.commit(pending.apply_diagnostics(vec![
            PluginDiagnostic::surface_registration_failed(
                plugin_id.clone(),
                SurfaceRegistrationError::DuplicateSurfaceKey {
                    surface_key: "duplicate".into(),
                },
            ),
        ]));

        assert_eq!(result.deltas.len(), 1);
        assert!(result.deltas[0].is_removed());
        assert_eq!(result.deltas[0].old.as_ref(), Some(&old_descriptor));
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].plugin_id(), Some(&plugin_id));
        assert_eq!(
            result.diagnostics[0].previous.as_ref(),
            Some(&old_descriptor)
        );
        assert_eq!(
            result.diagnostics[0].attempted.as_ref(),
            Some(&new_descriptor)
        );
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

    #[test]
    fn initialize_reports_instantiation_failure_and_skips_snapshot() {
        let provider = DemoFactoryProvider::new(FactoryVariant::Err, "r1");
        let mut manager = PluginManager::new(vec![Box::new(provider)]);
        let mut registry = PluginRegistry::new();

        let result = manager.initialize(&mut registry, |_, _| vec![]).unwrap();

        assert!(result.deltas.is_empty());
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics[0].plugin_id(),
            Some(&PluginId("demo".to_string()))
        );
        assert!(matches!(
            result.diagnostics[0].kind,
            PluginDiagnosticKind::InstantiationFailed
        ));
        assert!(result.diagnostics[0].previous.is_none());
        assert_eq!(
            result.diagnostics[0]
                .attempted
                .as_ref()
                .map(|descriptor| descriptor.revision.0.as_str()),
            Some("r1")
        );
        assert!(!registry.contains_plugin(&PluginId("demo".to_string())));
        assert!(
            manager
                .snapshot()
                .winner(&PluginId("demo".to_string()))
                .is_none()
        );
    }

    #[test]
    fn reload_reports_instantiation_failure_and_keeps_old_winner() {
        let provider = DemoFactoryProvider::new(FactoryVariant::Ok, "r1");
        let mut manager = PluginManager::new(vec![Box::new(provider.clone())]);
        let mut registry = PluginRegistry::new();

        let initial = manager.initialize(&mut registry, |_, _| vec![]).unwrap();
        assert!(initial.diagnostics.is_empty());
        assert_eq!(
            manager
                .snapshot()
                .winner(&PluginId("demo".to_string()))
                .map(|descriptor| descriptor.revision.0.as_str()),
            Some("r1")
        );

        provider.set_state(FactoryVariant::Err, "r2");
        let result = manager
            .reload(&mut registry, &AppState::default(), |_, _| vec![])
            .unwrap();

        assert!(result.deltas.is_empty());
        assert_eq!(result.diagnostics.len(), 1);
        assert!(matches!(
            result.diagnostics[0].kind,
            PluginDiagnosticKind::InstantiationFailed
        ));
        assert_eq!(
            result.diagnostics[0]
                .previous
                .as_ref()
                .map(|descriptor| descriptor.revision.0.as_str()),
            Some("r1")
        );
        assert_eq!(
            result.diagnostics[0]
                .attempted
                .as_ref()
                .map(|descriptor| descriptor.revision.0.as_str()),
            Some("r2")
        );
        assert!(registry.contains_plugin(&PluginId("demo".to_string())));
        assert_eq!(
            manager
                .snapshot()
                .winner(&PluginId("demo".to_string()))
                .map(|descriptor| descriptor.revision.0.as_str()),
            Some("r1")
        );
    }

    #[test]
    fn initialize_reports_provider_collect_failure_but_keeps_other_winners() {
        let good_provider = DemoFactoryProvider::new(FactoryVariant::Ok, "r1");
        let mut manager =
            PluginManager::new(vec![Box::new(FailingProvider), Box::new(good_provider)]);
        let mut registry = PluginRegistry::new();

        let result = manager.initialize(&mut registry, |_, _| vec![]).unwrap();

        assert_eq!(result.diagnostics.len(), 1);
        assert!(matches!(
            result.diagnostics[0].kind,
            PluginDiagnosticKind::ProviderCollectFailed
        ));
        assert_eq!(
            result.diagnostics[0].provider_name(),
            Some("failing-provider")
        );
        assert!(result.diagnostics[0].plugin_id().is_none());
        assert_eq!(result.deltas.len(), 1);
        assert!(result.deltas[0].is_added());
        assert!(registry.contains_plugin(&PluginId("demo".to_string())));
        assert_eq!(
            manager
                .snapshot()
                .winner(&PluginId("demo".to_string()))
                .map(|descriptor| descriptor.revision.0.as_str()),
            Some("r1")
        );
    }
}
