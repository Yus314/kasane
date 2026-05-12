//! Built-in plugins that ship with Kasane.
//!
//! These implement default policies (input handling, status display, etc.)
//! as lowest-priority plugins, allowing user plugins to override them.

mod diagnostics;
mod diagnostics_panel;
mod shadow_cursor;

use kasane_core::plugin::{PluginBridge, PluginFactory, builtin_plugin};
use kasane_core::render::view::info::BuiltinInfoPlugin;
use kasane_core::render::view::menu::BuiltinMenuPlugin;
use std::sync::Arc;

/// Collect built-in plugin factories for registration.
///
/// These are added to the provider list alongside WASM and host plugins.
/// Built-in plugins have the lowest priority rank so user plugins override them.
pub fn builtin_plugin_factories() -> Vec<Arc<dyn PluginFactory>> {
    vec![
        builtin_plugin("builtin-menu", "kasane.builtin.menu", || {
            PluginBridge::new(BuiltinMenuPlugin)
        }),
        builtin_plugin("builtin-info", "kasane.builtin.info", || {
            PluginBridge::new(BuiltinInfoPlugin)
        }),
        builtin_plugin("builtin-diagnostics", "kasane.builtin.diagnostics", || {
            PluginBridge::new(diagnostics::BuiltinDiagnosticsPlugin)
        }),
        builtin_plugin(
            "builtin-diagnostics-panel",
            "kasane.builtin.diagnostics_panel",
            || PluginBridge::new(diagnostics_panel::BuiltinDiagnosticsPanelPlugin),
        ),
        builtin_plugin(
            "builtin-shadow-cursor",
            "kasane.builtin.shadow_cursor",
            || PluginBridge::new(shadow_cursor::BuiltinShadowCursorPlugin),
        ),
    ]
}
