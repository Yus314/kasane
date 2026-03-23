//! Paint hook ownership tracking for the TUI event loop.

use std::collections::BTreeMap;

use kasane_core::plugin::{PaintHook, PluginId, PluginRuntime};

#[derive(Default)]
pub(crate) struct PaintHookState {
    hooks: Vec<Box<dyn PaintHook>>,
    owner_ranges: BTreeMap<PluginId, std::ops::Range<usize>>,
}

impl PaintHookState {
    pub(crate) fn from_registry(registry: &PluginRuntime) -> Self {
        let mut state = Self::default();
        let view = registry.view();
        state.rebuild_from_grouped(
            registry,
            view.paint_hook_owners_in_order()
                .into_iter()
                .map(|owner| {
                    let hooks = view.collect_paint_hooks_for_owner(&owner);
                    (owner, hooks)
                })
                .collect(),
        );
        state
    }

    pub(crate) fn hooks(&self) -> &[Box<dyn PaintHook>] {
        &self.hooks
    }

    pub(crate) fn reconcile(
        &mut self,
        registry: &PluginRuntime,
        deltas: &[kasane_core::plugin::AppliedWinnerDelta],
        diagnostics: &[kasane_core::plugin::PluginDiagnostic],
    ) {
        if deltas.is_empty() && diagnostics.is_empty() {
            return;
        }

        let mut grouped = self.take_grouped();
        let mut changed_owners = BTreeMap::<PluginId, ()>::new();
        for delta in deltas {
            changed_owners.insert(delta.id.clone(), ());
        }
        for diagnostic in diagnostics {
            if let Some(plugin_id) = diagnostic.plugin_id() {
                changed_owners.insert(plugin_id.clone(), ());
            }
        }

        for plugin_id in changed_owners.keys() {
            grouped.remove(plugin_id);
        }
        for plugin_id in changed_owners.keys() {
            if diagnostics.iter().any(|d| d.plugin_id() == Some(plugin_id))
                || !registry.contains_plugin(plugin_id)
            {
                continue;
            }
            let hooks = registry.view().collect_paint_hooks_for_owner(plugin_id);
            if !hooks.is_empty() {
                grouped.insert(plugin_id.clone(), hooks);
            }
        }

        self.rebuild_from_grouped(registry, grouped);
    }

    fn take_grouped(&mut self) -> BTreeMap<PluginId, Vec<Box<dyn PaintHook>>> {
        let old_hooks = std::mem::take(&mut self.hooks);
        let old_ranges = std::mem::take(&mut self.owner_ranges);
        let mut entries: Vec<_> = old_ranges.into_iter().collect();
        entries.sort_by_key(|(_, range)| range.start);
        let mut hooks_iter = old_hooks.into_iter();
        let mut grouped = BTreeMap::new();
        for (owner, range) in entries {
            let len = range.end.saturating_sub(range.start);
            grouped.insert(owner, hooks_iter.by_ref().take(len).collect());
        }
        grouped
    }

    fn rebuild_from_grouped(
        &mut self,
        registry: &PluginRuntime,
        mut grouped: BTreeMap<PluginId, Vec<Box<dyn PaintHook>>>,
    ) {
        let mut hooks = Vec::new();
        let mut owner_ranges = BTreeMap::new();
        for owner in registry.view().paint_hook_owners_in_order() {
            let Some(owner_hooks) = grouped.remove(&owner) else {
                continue;
            };
            let start = hooks.len();
            hooks.extend(owner_hooks);
            owner_ranges.insert(owner, start..hooks.len());
        }
        self.hooks = hooks;
        self.owner_ranges = owner_ranges;
    }
}
