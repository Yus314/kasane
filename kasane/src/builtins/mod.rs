//! Built-in plugins that ship with Kasane.
//!
//! These implement default policies (input handling, status display, etc.)
//! as lowest-priority plugins, allowing user plugins to override them.

mod diagnostics;

use kasane_core::plugin::{PluginFactory, builtin_plugin};
use std::sync::Arc;

/// Collect built-in plugin factories for registration.
///
/// These are added to the provider list alongside WASM and host plugins.
/// Built-in plugins have the lowest priority rank so user plugins override them.
pub fn builtin_plugin_factories() -> Vec<Arc<dyn PluginFactory>> {
    vec![builtin_plugin(
        "builtin-diagnostics",
        "kasane.builtin.diagnostics",
        || diagnostics::BuiltinDiagnosticsPlugin,
    )]
}
