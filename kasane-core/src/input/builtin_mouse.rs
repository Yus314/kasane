//! Built-in plugin for mouse-to-Kakoune fallback.

use crate::input::{self, MouseEvent};
use crate::plugin::{
    AppView, Command, FrameworkAccess, PluginBackend, PluginCapabilities, PluginId,
};

/// Built-in plugin that forwards unhandled mouse events to Kakoune.
///
/// This replaces the hardcoded `mouse_to_kakoune()` call in `update.rs`,
/// allowing user plugins to suppress the default mouse-to-Kakoune behavior
/// by registering their own `MOUSE_FALLBACK` handler.
pub struct BuiltinMouseFallbackPlugin;

crate::impl_migrated_caps_default!(BuiltinMouseFallbackPlugin);

impl PluginBackend for BuiltinMouseFallbackPlugin {
    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.mouse_fallback".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::MOUSE_FALLBACK
    }

    fn handle_mouse_fallback(
        &mut self,
        event: &MouseEvent,
        scroll_amount: i32,
        state: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        let app = state.as_app_state();
        let req = input::mouse_to_kakoune(
            event,
            scroll_amount,
            app.runtime.display_map.as_deref(),
            app.runtime.display_scroll_offset,
            app.runtime.segment_map.as_deref(),
        )?;
        Some(vec![Command::SendToKakoune(req)])
    }
}
