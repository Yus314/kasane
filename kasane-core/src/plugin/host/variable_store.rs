//! Plugin variable store — collects variables exposed by plugins for widget resolution.

use std::collections::HashMap;

use compact_str::CompactString;

use super::super::PluginId;
use crate::widget::types::Value;

/// One entry in the store: a value plus the plugin that wrote it. The owner
/// is recorded so we can clean up entries when their plugin unloads (the
/// variable name itself is not namespaced by plugin id, so the only reliable
/// way to attribute a variable is at write time).
#[derive(Debug, Clone)]
struct VariableEntry {
    value: Value,
    owner: PluginId,
}

/// Stores variables exposed by plugins via `Command::ExposeVariable`.
///
/// Widget templates can reference these as `{plugin.<name>}`.
#[derive(Default, Debug)]
pub struct PluginVariableStore {
    vars: HashMap<CompactString, VariableEntry>,
}

impl PluginVariableStore {
    /// Set a variable exposed by `owner`. Overwrites previous values
    /// regardless of who owned them — this matches the prior behavior and
    /// keeps the latest writer authoritative.
    pub fn set(&mut self, name: &str, value: Value, owner: PluginId) {
        self.vars
            .insert(CompactString::from(name), VariableEntry { value, owner });
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.vars.get(name).map(|entry| &entry.value)
    }

    /// Remove every variable owned by `plugin_id`. Called by the plugin
    /// runtime when a plugin is unloaded so its exposed variables don't
    /// outlive the plugin instance.
    pub fn clear_for_plugin(&mut self, plugin_id: &PluginId) {
        self.vars.retain(|_, entry| entry.owner != *plugin_id);
    }

    /// Clear all variables from a specific plugin (by name prefix).
    /// Retained for callers that want to namespace by string convention.
    pub fn clear_prefix(&mut self, prefix: &str) {
        self.vars.retain(|k, _| !k.starts_with(prefix));
    }

    /// Clear all variables.
    pub fn clear_all(&mut self) {
        self.vars.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(s: &str) -> PluginId {
        PluginId::from(s)
    }

    #[test]
    fn clear_for_plugin_removes_only_owner_entries() {
        let mut store = PluginVariableStore::default();
        store.set("a", Value::Str("1".into()), pid("alpha"));
        store.set("b", Value::Str("2".into()), pid("beta"));
        store.set("c", Value::Str("3".into()), pid("alpha"));

        store.clear_for_plugin(&pid("alpha"));

        assert!(store.get("a").is_none());
        assert!(store.get("b").is_some());
        assert!(store.get("c").is_none());
    }

    #[test]
    fn clear_for_plugin_is_noop_for_unknown() {
        let mut store = PluginVariableStore::default();
        store.set("a", Value::Str("1".into()), pid("alpha"));
        store.clear_for_plugin(&pid("ghost"));
        assert!(store.get("a").is_some());
    }

    #[test]
    fn set_overwrites_regardless_of_previous_owner() {
        let mut store = PluginVariableStore::default();
        store.set("a", Value::Str("first".into()), pid("alpha"));
        store.set("a", Value::Str("second".into()), pid("beta"));
        assert_eq!(store.get("a"), Some(&Value::Str("second".into())));
        store.clear_for_plugin(&pid("alpha"));
        // alpha no longer owns "a", so the entry survives.
        assert!(store.get("a").is_some());
        store.clear_for_plugin(&pid("beta"));
        assert!(store.get("a").is_none());
    }
}
