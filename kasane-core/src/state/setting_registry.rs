//! Data-driven core setting registry.
//!
//! Replaces the hardcoded `apply_set_config()` match statement with a
//! registry of core settings. Each entry maps a key string to an apply
//! function that modifies `AppState` and returns the dirty flags.
//!
//! This module enables:
//! - Data-driven setting application (no match arms to maintain)
//! - Discoverability of core settings (keys, descriptions)
//! - Future: plugin-extensible settings via the same registry pattern

use std::collections::HashMap;
use std::sync::LazyLock;

use super::AppState;
use crate::state::DirtyFlags;

/// Apply function signature: takes `&mut AppState` and a string value,
/// returns dirty flags on success or `DirtyFlags::empty()` on parse failure.
type ApplyStrFn = fn(&mut AppState, &str) -> DirtyFlags;

/// A registered core setting.
struct CoreSettingEntry {
    apply: ApplyStrFn,
}

/// Registry of known core settings.
pub(crate) struct CoreSettingRegistry {
    entries: HashMap<&'static str, CoreSettingEntry>,
}

impl CoreSettingRegistry {
    fn new() -> Self {
        let mut reg = Self {
            entries: HashMap::new(),
        };
        register_all(&mut reg);
        reg
    }

    fn register(&mut self, key: &'static str, apply: ApplyStrFn) {
        self.entries.insert(key, CoreSettingEntry { apply });
    }

    /// Try to apply a config key/value pair through the registry.
    ///
    /// Returns `Some(dirty_flags)` if the key was handled, `None` if unknown.
    pub(crate) fn apply(&self, state: &mut AppState, key: &str, value: &str) -> Option<DirtyFlags> {
        self.entries
            .get(key)
            .map(|entry| (entry.apply)(state, value))
    }

    /// Returns an iterator over all registered core setting keys.
    #[cfg(test)]
    fn keys(&self) -> impl Iterator<Item = &&'static str> {
        self.entries.keys()
    }
}

/// Global core setting registry, initialized on first access.
pub(crate) static REGISTRY: LazyLock<CoreSettingRegistry> = LazyLock::new(CoreSettingRegistry::new);

/// Register all core settings.
fn register_all(reg: &mut CoreSettingRegistry) {
    reg.register("shadow_enabled", |state, value| {
        state.config.shadow_enabled = value == "true";
        DirtyFlags::OPTIONS
    });

    reg.register("padding_char", |state, value| {
        state.config.padding_char = value.to_string();
        DirtyFlags::BUFFER_CONTENT
    });

    reg.register("search_dropdown", |state, value| {
        state.config.search_dropdown = value == "true";
        DirtyFlags::OPTIONS
    });

    reg.register("status_at_top", |state, value| {
        state.config.status_at_top = value == "true";
        DirtyFlags::OPTIONS
    });

    reg.register("cursor.secondary_blend", |state, value| {
        if let Ok(ratio) = value.parse::<f32>() {
            state.config.secondary_blend_ratio = ratio.clamp(0.0, 1.0);
            DirtyFlags::BUFFER_CONTENT
        } else {
            DirtyFlags::empty()
        }
    });

    reg.register("scrollbar.thumb", |state, value| {
        state.config.scrollbar_thumb = value.to_string();
        DirtyFlags::MENU_STRUCTURE
    });

    reg.register("scrollbar.track", |state, value| {
        state.config.scrollbar_track = value.to_string();
        DirtyFlags::MENU_STRUCTURE
    });

    reg.register("divider.vertical", |state, value| {
        state.config.divider_vertical = value.to_string();
        DirtyFlags::OPTIONS
    });

    reg.register("divider.horizontal", |state, value| {
        state.config.divider_horizontal = value.to_string();
        DirtyFlags::OPTIONS
    });

    reg.register("scroll.edge_margin", |state, value| {
        if let Ok(v) = value.parse::<u16>() {
            state.config.scroll_edge_margin = v;
            DirtyFlags::OPTIONS
        } else {
            DirtyFlags::empty()
        }
    });

    reg.register("scroll.info_step", |state, value| {
        if let Ok(v) = value.parse::<u16>() {
            state.config.info_scroll_step = v;
            DirtyFlags::OPTIONS
        } else {
            DirtyFlags::empty()
        }
    });

    reg.register("render.newline_display", |state, value| {
        state.config.newline_display = value.to_string();
        DirtyFlags::OPTIONS
    });

    reg.register("render.truncation_char", |state, value| {
        state.config.truncation_char = value.to_string();
        DirtyFlags::OPTIONS
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_handles_known_key() {
        let mut state = AppState::default();
        let result = REGISTRY.apply(&mut state, "shadow_enabled", "false");
        assert!(result.is_some());
        assert!(!state.config.shadow_enabled);
    }

    #[test]
    fn registry_returns_none_for_unknown_key() {
        let mut state = AppState::default();
        let result = REGISTRY.apply(&mut state, "nonexistent_key", "value");
        assert!(result.is_none());
    }

    #[test]
    fn registry_parses_numeric_settings() {
        let mut state = AppState::default();
        let result = REGISTRY.apply(&mut state, "scroll.edge_margin", "5");
        assert!(result.is_some());
        assert_eq!(state.config.scroll_edge_margin, 5);
    }

    #[test]
    fn registry_returns_empty_flags_on_parse_failure() {
        let mut state = AppState::default();
        let original = state.config.scroll_edge_margin;
        let result = REGISTRY.apply(&mut state, "scroll.edge_margin", "not_a_number");
        assert_eq!(result, Some(DirtyFlags::empty()));
        assert_eq!(state.config.scroll_edge_margin, original);
    }

    #[test]
    fn registry_has_all_core_settings() {
        let expected_keys = [
            "shadow_enabled",
            "padding_char",
            "search_dropdown",
            "status_at_top",
            "cursor.secondary_blend",
            "scrollbar.thumb",
            "scrollbar.track",
            "divider.vertical",
            "divider.horizontal",
            "scroll.edge_margin",
            "scroll.info_step",
            "render.newline_display",
            "render.truncation_char",
        ];
        for key in &expected_keys {
            assert!(
                REGISTRY.entries.contains_key(key),
                "missing core setting: {key}"
            );
        }
        assert_eq!(REGISTRY.entries.len(), expected_keys.len());
    }
}
