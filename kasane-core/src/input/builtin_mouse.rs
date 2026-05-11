//! Built-in plugin for mouse-to-Kakoune fallback.

use crate::input;
use crate::plugin::{FrameworkAccess, HandlerRegistry, KakouneSideCommand, Plugin, PluginId};

/// Built-in plugin that forwards unhandled mouse events to Kakoune.
///
/// This replaces the hardcoded `mouse_to_kakoune()` call in `update.rs`,
/// allowing user plugins to suppress the default mouse-to-Kakoune behavior
/// by registering their own `MOUSE_FALLBACK` handler.
pub struct BuiltinMouseFallbackPlugin;

impl Plugin for BuiltinMouseFallbackPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.mouse_fallback".into())
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        // Tier 1 (ADR-044): mouse-fallback only forwards Kakoune-side input;
        // it never spawns processes. `on_mouse_fallback_tier1` pins that
        // contract — a future edit that returns a `ProcessCommand` would
        // not compile.
        r.on_mouse_fallback_tier1(|_state, event, scroll_amount, view| {
            let app = view.as_app_state();
            let result = input::mouse_to_kakoune(
                event,
                scroll_amount,
                app.runtime.display_map.as_deref(),
                app.runtime.display_scroll_offset,
                app.runtime.segment_map.as_deref(),
            )
            .map(|req| vec![KakouneSideCommand::send_to_kakoune(req)]);
            ((), result)
        });
    }
}
