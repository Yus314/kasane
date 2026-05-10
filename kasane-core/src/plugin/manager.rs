use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::AppView;
use super::diagnostics::{PluginDiagnostic, PluginDiagnosticKind, PluginDiagnosticTarget};
use super::provider::{PluginRevision, ProviderConfigUpdate};
use super::setting::SettingValue;
use super::{Effects, PluginDescriptor, PluginFactory, PluginId, PluginProvider, PluginRuntime};

/// Number of consecutive activation failures for the same
/// `(plugin_id, revision)` pair before the [`PluginManager`] starts
/// suppressing retries.
const ACTIVATION_QUARANTINE_THRESHOLD: u32 = 3;
/// How long a quarantined `(plugin_id, revision)` is suppressed before
/// the manager will retry. Bumping the plugin's revision (i.e. the user
/// installs a new build) resets the quarantine immediately.
const ACTIVATION_QUARANTINE_COOLDOWN: Duration = Duration::from_secs(30);

/// Tracks repeated activation failures for one `(plugin_id, revision)`
/// pair so the manager can suppress retries after a threshold. Reset
/// implicitly when the revision changes (a fresh map entry is created).
#[derive(Debug, Clone, Copy)]
struct ActivationFailureState {
    consecutive: u32,
    last_failure: Instant,
}

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
    initial_settings: HashMap<PluginId, HashMap<String, SettingValue>>,
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
    pub bootstrap: Effects,
    pub deltas: Vec<AppliedWinnerDelta>,
    pub diagnostics: Vec<PluginDiagnostic>,
    /// Per-plugin initial settings to apply to AppState after plugin loading.
    pub settings_to_apply: HashMap<PluginId, HashMap<String, SettingValue>>,
}

impl PluginApplyResult {
    pub fn active_set_changed(&self) -> bool {
        !self.deltas.is_empty()
    }

    /// Apply initial plugin settings to AppState.
    /// Should be called after plugin loading to seed per-plugin settings.
    pub fn apply_settings(&self, state: &mut crate::state::AppState) {
        for (plugin_id, settings) in &self.settings_to_apply {
            state
                .config
                .plugin_settings
                .entry(plugin_id.clone())
                .or_default()
                .extend(settings.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
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
    initial_settings: HashMap<PluginId, HashMap<String, SettingValue>>,
}

enum PluginApplyMode<'a> {
    Initial,
    Reload(&'a AppView<'a>),
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
    pre_render_hooks: Vec<Box<dyn crate::event_loop::PreRenderHook>>,
    /// Per-`(plugin_id, revision)` activation-failure tally. Used to
    /// suppress retries after [`ACTIVATION_QUARANTINE_THRESHOLD`]
    /// consecutive failures of the same revision until
    /// [`ACTIVATION_QUARANTINE_COOLDOWN`] elapses or the revision
    /// changes (the next attempt for a *new* revision starts with a
    /// fresh map entry, so a user-initiated rebuild re-arms the system
    /// immediately).
    activation_failures: HashMap<(PluginId, PluginRevision), ActivationFailureState>,
}

impl PluginManager {
    pub fn new(providers: Vec<Box<dyn PluginProvider>>) -> Self {
        Self {
            providers,
            previous: ResolvedPluginSnapshot::default(),
            pre_render_hooks: Vec::new(),
            activation_failures: HashMap::new(),
        }
    }

    /// Add a pre-render hook that runs before each Salsa sync.
    ///
    /// Hooks receive `&mut AppState` and can update runtime fields (e.g.
    /// syntax provider) before the render frame.
    pub fn add_pre_render_hook(&mut self, hook: Box<dyn crate::event_loop::PreRenderHook>) {
        self.pre_render_hooks.push(hook);
    }

    /// Propagate dynamic configuration changes to all registered providers.
    ///
    /// Should be called when the user-facing configuration changes (e.g.,
    /// after a `kasane.kdl` hot-reload) and before [`Self::reload`] so that
    /// the next collect cycle reflects the new configuration. Providers that
    /// don't depend on dynamic config (e.g. [`super::StaticPluginProvider`])
    /// are no-ops.
    ///
    /// Returns diagnostics for providers whose `update_config` failed; the
    /// other providers are still updated. The caller decides how to surface
    /// these diagnostics (overlay, log, etc.).
    pub fn update_provider_config(
        &self,
        plugins: &crate::config::PluginsConfig,
        settings: &std::collections::HashMap<
            String,
            std::collections::HashMap<String, SettingValue>,
        >,
    ) -> Vec<PluginDiagnostic> {
        let update = ProviderConfigUpdate { plugins, settings };
        let mut diagnostics = Vec::new();
        for provider in &self.providers {
            if let Err(err) = provider.update_config(update) {
                diagnostics.push(PluginDiagnostic::provider_collect_failed(
                    provider.name(),
                    format!("update_config: {err}"),
                ));
            }
        }
        diagnostics
    }

    /// Run all pre-render hooks on the given state.
    pub fn run_pre_render_hooks(&mut self, state: &mut crate::state::AppState) {
        for hook in &mut self.pre_render_hooks {
            hook.pre_render(state);
        }
    }

    fn plan_initial(&self) -> Result<PluginApplyPlan> {
        let catalog = self.collect_and_resolve()?;
        let next_snapshot = catalog.snapshot();
        let initial_settings = catalog.initial_settings;
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
            initial_settings,
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
            initial_settings: catalog.initial_settings,
        })
    }

    fn apply_plan(
        &mut self,
        registry: &mut PluginRuntime,
        plan: PluginApplyPlan,
        mode: PluginApplyMode<'_>,
    ) -> Result<PendingPluginCommit> {
        let PluginApplyPlan {
            removals,
            upserts,
            mut next_snapshot,
            initial_settings,
            ..
        } = plan;
        let mut result = PluginApplyResult {
            settings_to_apply: initial_settings,
            ..PluginApplyResult::default()
        };

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
            // Circuit breaker: if this exact (plugin_id, revision) has
            // failed activation repeatedly within the cooldown window,
            // skip the attempt silently. Bumping the revision (i.e.
            // user installs a new build) creates a new map key and
            // re-arms the breaker. See ACTIVATION_QUARANTINE_THRESHOLD.
            let breaker_key = (upsert.id.clone(), upsert.new.revision.clone());
            if let Some(state) = self.activation_failures.get(&breaker_key)
                && state.consecutive >= ACTIVATION_QUARANTINE_THRESHOLD
                && state.last_failure.elapsed() < ACTIVATION_QUARANTINE_COOLDOWN
            {
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

            let plugin = match upsert.factory.create() {
                Ok(plugin) => plugin,
                Err(err) => {
                    let entry = self.activation_failures.entry(breaker_key).or_insert(
                        ActivationFailureState {
                            consecutive: 0,
                            last_failure: Instant::now(),
                        },
                    );
                    entry.consecutive = entry.consecutive.saturating_add(1);
                    entry.last_failure = Instant::now();
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
            // Success: clear any prior failure record so future failures
            // get the full threshold of warnings before quarantine.
            self.activation_failures.remove(&breaker_key);
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
        registry: &mut PluginRuntime,
        collect_diagnostics: F,
    ) -> Result<PluginApplyResult>
    where
        F: FnOnce(&PluginApplyResult, &mut PluginRuntime) -> Vec<PluginDiagnostic>,
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
        registry: &mut PluginRuntime,
        state: &AppView<'_>,
        collect_diagnostics: F,
    ) -> Result<PluginApplyResult>
    where
        F: FnOnce(&PluginApplyResult, &mut PluginRuntime) -> Vec<PluginDiagnostic>,
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
        let mut initial_settings: HashMap<PluginId, HashMap<String, SettingValue>> = HashMap::new();
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
            initial_settings.extend(collect.initial_settings);
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
            initial_settings,
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
    use crate::state::AppState;
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
                initial_settings: HashMap::new(),
            })
        }
    }

    #[test]
    fn commit_discards_failed_added_plugin() {
        let plugin_id = PluginId("demo".to_string());
        let descriptor = host_descriptor("demo", "r1");
        let pending = PendingPluginCommit {
            result: PluginApplyResult {
                bootstrap: Effects::default(),
                deltas: vec![AppliedWinnerDelta {
                    id: plugin_id.clone(),
                    old: None,
                    new: Some(descriptor.clone()),
                }],
                diagnostics: vec![],
                settings_to_apply: HashMap::new(),
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
                bootstrap: Effects::default(),
                deltas: vec![AppliedWinnerDelta {
                    id: plugin_id.clone(),
                    old: Some(old_descriptor.clone()),
                    new: Some(new_descriptor.clone()),
                }],
                diagnostics: vec![],
                settings_to_apply: HashMap::new(),
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
        let mut registry = PluginRuntime::new();

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
        let mut registry = PluginRuntime::new();

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
        let state = AppState::default();
        let result = manager
            .reload(&mut registry, &AppView::new(&state), |_, _| vec![])
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
        let mut registry = PluginRuntime::new();

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

    #[derive(Default)]
    struct ConfigCapturingProvider {
        captured: Arc<Mutex<Option<crate::config::PluginsConfig>>>,
        captured_settings: Arc<Mutex<HashMap<String, HashMap<String, SettingValue>>>>,
        fail: bool,
    }

    impl PluginProvider for ConfigCapturingProvider {
        fn name(&self) -> &'static str {
            "config-capturing"
        }

        fn collect(&self) -> Result<PluginCollect> {
            Ok(PluginCollect::default())
        }

        fn update_config(&self, update: ProviderConfigUpdate<'_>) -> Result<()> {
            if self.fail {
                return Err(anyhow!("update_config refused"));
            }
            *self.captured.lock().unwrap() = Some(update.plugins.clone());
            *self.captured_settings.lock().unwrap() = update.settings.clone();
            Ok(())
        }
    }

    #[test]
    fn update_provider_config_propagates_to_all_providers() {
        let captured = Arc::new(Mutex::new(None));
        let captured_settings = Arc::new(Mutex::new(HashMap::new()));
        let provider = ConfigCapturingProvider {
            captured: captured.clone(),
            captured_settings: captured_settings.clone(),
            fail: false,
        };
        let manager = PluginManager::new(vec![Box::new(provider)]);

        let mut plugins = crate::config::PluginsConfig::default();
        plugins.enabled.push("cursor_line".to_string());
        let mut settings: HashMap<String, HashMap<String, SettingValue>> = HashMap::new();
        settings.insert(
            "cursor_line".to_string(),
            HashMap::from([("intensity".to_string(), SettingValue::Integer(7))]),
        );

        let diags = manager.update_provider_config(&plugins, &settings);
        assert!(diags.is_empty());

        let snapshot = captured.lock().unwrap();
        assert_eq!(snapshot.as_ref().unwrap().enabled, vec!["cursor_line"]);
        let settings_snapshot = captured_settings.lock().unwrap();
        assert_eq!(
            settings_snapshot
                .get("cursor_line")
                .and_then(|m| m.get("intensity")),
            Some(&SettingValue::Integer(7))
        );
    }

    #[test]
    fn update_provider_config_collects_diagnostics_from_failing_provider() {
        let provider = ConfigCapturingProvider {
            fail: true,
            ..Default::default()
        };
        let manager = PluginManager::new(vec![Box::new(provider)]);

        let plugins = crate::config::PluginsConfig::default();
        let settings: HashMap<String, HashMap<String, SettingValue>> = HashMap::new();
        let diags = manager.update_provider_config(&plugins, &settings);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("update_config"));
    }

    #[test]
    fn circuit_breaker_quarantines_after_threshold() {
        // Reload N times with the same revision and a failing factory:
        // the first ACTIVATION_QUARANTINE_THRESHOLD attempts produce
        // diagnostics; subsequent attempts within the cooldown window
        // are silent (no new diagnostic). Once the user updates the
        // plugin (revision bump), the breaker resets and surfacing
        // resumes immediately.
        let provider = DemoFactoryProvider::new(FactoryVariant::Err, "broken-r1");
        let mut manager = PluginManager::new(vec![Box::new(provider.clone())]);
        let mut registry = PluginRuntime::new();

        // Initial activation: 1st failure recorded + diagnostic emitted.
        let r1 = manager.initialize(&mut registry, |_, _| vec![]).unwrap();
        assert_eq!(r1.diagnostics.len(), 1, "first failure surfaces");

        let state = AppState::default();
        // Reloads 2 and 3 still under threshold → diagnostics emitted.
        for attempt in 2..=ACTIVATION_QUARANTINE_THRESHOLD {
            let r = manager
                .reload(&mut registry, &AppView::new(&state), |_, _| vec![])
                .unwrap();
            assert_eq!(
                r.diagnostics.len(),
                1,
                "attempt {attempt} should still surface a diagnostic"
            );
        }

        // Now over threshold and within cooldown: must be silent.
        let suppressed = manager
            .reload(&mut registry, &AppView::new(&state), |_, _| vec![])
            .unwrap();
        assert!(
            suppressed.diagnostics.is_empty(),
            "quarantine must suppress further diagnostics, got {:?}",
            suppressed.diagnostics
        );

        // Revision bump: same id, fresh map key → quarantine cleared.
        provider.set_state(FactoryVariant::Err, "broken-r2");
        let after_bump = manager
            .reload(&mut registry, &AppView::new(&state), |_, _| vec![])
            .unwrap();
        assert_eq!(
            after_bump.diagnostics.len(),
            1,
            "new revision must re-arm the breaker, got {:?}",
            after_bump.diagnostics
        );
    }

    #[test]
    fn circuit_breaker_resets_on_success() {
        // After a streak of failures, a successful activation must
        // clear the breaker so future failures get the full threshold
        // of warnings again.
        let provider = DemoFactoryProvider::new(FactoryVariant::Err, "r1");
        let mut manager = PluginManager::new(vec![Box::new(provider.clone())]);
        let mut registry = PluginRuntime::new();

        let _ = manager.initialize(&mut registry, |_, _| vec![]).unwrap();
        assert_eq!(manager.activation_failures.len(), 1);

        // Flip the factory to succeed and reload (still r1).
        provider.set_state(FactoryVariant::Ok, "r1");
        let state = AppState::default();
        let _ = manager
            .reload(&mut registry, &AppView::new(&state), |_, _| vec![])
            .unwrap();
        assert!(
            manager.activation_failures.is_empty(),
            "successful activation must clear the breaker entry"
        );
    }
}
