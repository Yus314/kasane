//! Plugin variable store — collects variables exposed by plugins for widget resolution.

use std::collections::HashMap;

use compact_str::CompactString;

use crate::widget::types::Value;

/// Stores variables exposed by plugins via `Command::ExposeVariable`.
///
/// Widget templates can reference these as `{plugin.<name>}`.
#[derive(Default, Debug)]
pub struct PluginVariableStore {
    vars: HashMap<CompactString, Value>,
}

impl PluginVariableStore {
    pub fn set(&mut self, name: &str, value: Value) {
        self.vars.insert(CompactString::from(name), value);
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.vars.get(name)
    }

    /// Clear all variables from a specific plugin (by prefix).
    pub fn clear_prefix(&mut self, prefix: &str) {
        self.vars.retain(|k, _| !k.starts_with(prefix));
    }

    /// Clear all variables.
    pub fn clear_all(&mut self) {
        self.vars.clear();
    }
}
