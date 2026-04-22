//! Built-in plugins that ship with Kasane.
//!
//! These implement default policies (input handling, status display, etc.)
//! as lowest-priority plugins, allowing user plugins to override them.

mod diagnostics;
mod info;
mod menu;
mod shadow_cursor;

use kasane_core::plugin::{PluginFactory, builtin_plugin};
use std::sync::Arc;

/// Collect built-in plugin factories for registration.
///
/// These are added to the provider list alongside WASM and host plugins.
/// Built-in plugins have the lowest priority rank so user plugins override them.
pub fn builtin_plugin_factories() -> Vec<Arc<dyn PluginFactory>> {
    vec![
        builtin_plugin("builtin-menu", "kasane.builtin.menu", || {
            menu::BuiltinMenuPlugin
        }),
        builtin_plugin("builtin-info", "kasane.builtin.info", || {
            info::BuiltinInfoPlugin
        }),
        builtin_plugin("builtin-diagnostics", "kasane.builtin.diagnostics", || {
            diagnostics::BuiltinDiagnosticsPlugin
        }),
        builtin_plugin(
            "builtin-shadow-cursor",
            "kasane.builtin.shadow_cursor",
            || shadow_cursor::BuiltinShadowCursorPlugin,
        ),
    ]
}
