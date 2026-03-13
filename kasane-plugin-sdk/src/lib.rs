//! SDK for writing Kasane WASM plugins.
//!
//! Provides constants, helper macros, and the WIT interface definition
//! for building Kasane plugins targeting `wasm32-wasip2`.
//!
//! # Quick Start
//!
//! ```ignore
//! // Cargo.toml:
//! // [dependencies]
//! // kasane-plugin-sdk = { path = "../../kasane-plugin-sdk" }
//!
//! // src/lib.rs:
//! kasane_plugin_sdk::generate!("../../../kasane-plugin-sdk/wit");
//!
//! use exports::kasane::plugin::plugin_api::Guest;
//! use kasane::plugin::types::*;
//! use kasane::plugin::{host_state, element_builder};
//! use kasane_plugin_sdk::{slot, dirty};
//!
//! struct MyPlugin;
//!
//! impl Guest for MyPlugin {
//!     fn get_id() -> String { "my_plugin".into() }
//!
//!     fn contribute(s: u8) -> Option<ElementHandle> {
//!         if s != slot::BUFFER_LEFT { return None; }
//!         // ... build elements via element_builder ...
//!         None
//!     }
//!
//!     fn slot_deps(s: u8) -> u16 {
//!         kasane_plugin_sdk::route_slot_deps!(s, {
//!             slot::BUFFER_LEFT => dirty::BUFFER,
//!         })
//!     }
//!
//!     kasane_plugin_sdk::default_lifecycle!();
//!     kasane_plugin_sdk::default_line!();
//!     kasane_plugin_sdk::default_cache!();
//!     kasane_plugin_sdk::default_input!();
//!     kasane_plugin_sdk::default_overlay!();
//! }
//!
//! export!(MyPlugin);
//! ```

/// Slot indices matching `kasane_core::plugin::Slot`.
pub mod slot {
    pub const BUFFER_LEFT: u8 = 0;
    pub const BUFFER_RIGHT: u8 = 1;
    pub const ABOVE_BUFFER: u8 = 2;
    pub const BELOW_BUFFER: u8 = 3;
    pub const ABOVE_STATUS: u8 = 4;
    pub const STATUS_LEFT: u8 = 5;
    pub const STATUS_RIGHT: u8 = 6;
    pub const OVERLAY: u8 = 7;
    pub const COUNT: usize = 8;
}

/// DirtyFlags bit values matching `kasane_core::state::DirtyFlags`.
pub mod dirty {
    pub const BUFFER: u16 = 1 << 0;
    pub const STATUS: u16 = 1 << 1;
    pub const MENU_STRUCTURE: u16 = 1 << 2;
    pub const MENU_SELECTION: u16 = 1 << 3;
    pub const INFO: u16 = 1 << 4;
    pub const OPTIONS: u16 = 1 << 5;
    pub const MENU: u16 = MENU_STRUCTURE | MENU_SELECTION;
    pub const ALL: u16 = BUFFER | STATUS | MENU | INFO | OPTIONS;
}

/// Modifier key bitflags matching `kasane_core::input::Modifiers`.
pub mod modifiers {
    pub const CTRL: u8 = 0b0000_0001;
    pub const ALT: u8 = 0b0000_0010;
    pub const SHIFT: u8 = 0b0000_0100;
}

/// Bundled WIT interface definition (for reference/testing; not usable with proc macros).
pub const WIT: &str = include_str!("../wit/plugin.wit");

/// Generate Kasane plugin WIT bindings.
///
/// Pass the path to the SDK's `wit` directory (relative to the guest crate root).
/// The macro wraps `wit_bindgen::generate!` and also brings `wit_bindgen` into
/// scope via the SDK's re-export, so guests don't need a direct `wit-bindgen` dep.
///
/// # Example
/// ```ignore
/// kasane_plugin_sdk::generate!("../../../kasane-plugin-sdk/wit");
/// ```
///
/// The generated modules:
/// - `exports::kasane::plugin::plugin_api::Guest` — trait to implement
/// - `kasane::plugin::host_state` — host state query functions
/// - `kasane::plugin::element_builder` — element construction functions
/// - `kasane::plugin::types::*` — shared types (Face, Color, etc.)
/// Note: Guest crates must also depend on `wit-bindgen` directly, since
/// `wit_bindgen::generate!` generates code referencing `wit_bindgen` runtime types.
#[macro_export]
macro_rules! generate {
    ($wit_dir:literal) => {
        wit_bindgen::generate!({
            world: "kasane-plugin",
            path: $wit_dir,
        });
    };
}

/// Default lifecycle stubs (on_init, on_shutdown, on_state_changed).
///
/// Use inside a `Guest` trait impl to skip implementing unused lifecycle hooks.
/// For partial overrides, use `default_init!`, `default_shutdown!`, or
/// `default_state_changed!` individually.
#[macro_export]
macro_rules! default_lifecycle {
    () => {
        $crate::default_init!();
        $crate::default_shutdown!();
        $crate::default_state_changed!();
    };
}

/// Default on_init stub (returns empty command list).
#[macro_export]
macro_rules! default_init {
    () => {
        fn on_init() -> Vec<Command> {
            vec![]
        }
    };
}

/// Default on_shutdown stub (returns empty command list).
#[macro_export]
macro_rules! default_shutdown {
    () => {
        fn on_shutdown() -> Vec<Command> {
            vec![]
        }
    };
}

/// Default on_state_changed stub (returns empty command list).
#[macro_export]
macro_rules! default_state_changed {
    () => {
        fn on_state_changed(_dirty_flags: u16) -> Vec<Command> {
            vec![]
        }
    };
}

/// Default line decoration stub (contribute_line returns None).
#[macro_export]
macro_rules! default_line {
    () => {
        fn contribute_line(_line: u32) -> Option<LineDecoration> {
            None
        }
    };
}

/// Default caching stubs (state_hash returns 0, slot_deps returns ALL).
#[macro_export]
macro_rules! default_cache {
    () => {
        fn state_hash() -> u64 {
            0
        }
        fn slot_deps(_slot: u8) -> u16 {
            $crate::dirty::ALL
        }
    };
}

/// Default contribute stub (returns None for all slots).
#[macro_export]
macro_rules! default_contribute {
    () => {
        fn contribute(_slot: u8) -> Option<ElementHandle> {
            None
        }
    };
}

/// Default input handling stubs (handle_mouse, handle_key, observe_key, observe_mouse).
#[macro_export]
macro_rules! default_input {
    () => {
        fn handle_mouse(_event: MouseEvent, _id: InteractiveId) -> Option<Vec<Command>> {
            None
        }
        fn handle_key(_event: KeyEvent) -> Option<Vec<Command>> {
            None
        }
        fn observe_key(_event: KeyEvent) {}
        fn observe_mouse(_event: MouseEvent) {}
    };
}

/// Default overlay stub (contribute_overlay returns None).
#[macro_export]
macro_rules! default_overlay {
    () => {
        fn contribute_overlay() -> Option<Overlay> {
            None
        }
    };
}

/// Default menu transformation stub (returns None = no change).
#[macro_export]
macro_rules! default_menu_transform {
    () => {
        fn transform_menu_item(
            _item: Vec<Atom>,
            _index: u32,
            _selected: bool,
        ) -> Option<Vec<Atom>> {
            None
        }
    };
}

/// Default replacement stub (returns None for all targets).
#[macro_export]
macro_rules! default_replace {
    () => {
        fn replace(_target: ReplaceTarget) -> Option<ElementHandle> {
            None
        }
    };
}

/// Default decorator stub (passes through the element unchanged).
#[macro_export]
macro_rules! default_decorate {
    () => {
        fn decorate(_target: DecorateTarget, element: ElementHandle) -> ElementHandle {
            element
        }
    };
}

/// Default decorator priority stub (returns 0).
#[macro_export]
macro_rules! default_decorator_priority {
    () => {
        fn decorator_priority() -> u32 {
            0
        }
    };
}

/// Default cursor style override stub (returns None = no override).
#[macro_export]
macro_rules! default_cursor_style {
    () => {
        fn cursor_style_override() -> Option<u8> {
            None
        }
    };
}

/// Default named slot contribution stub (returns None).
#[macro_export]
macro_rules! default_named_slot {
    () => {
        fn contribute_named(_slot_name: String) -> Option<ElementHandle> {
            None
        }
    };
}

/// Default update stub (returns empty command list).
#[macro_export]
macro_rules! default_update {
    () => {
        fn update(_payload: Vec<u8>) -> Vec<Command> {
            vec![]
        }
    };
}

/// Route slot-based dispatch. Returns `None` for unmatched slots.
///
/// # Example
/// ```ignore
/// fn contribute(slot: u8) -> Option<ElementHandle> {
///     kasane_plugin_sdk::route_slots!(slot, {
///         slot::BUFFER_LEFT => {
///             Some(element_builder::create_text("hello", face))
///         },
///     })
/// }
/// ```
#[macro_export]
macro_rules! route_slots {
    ($slot:expr, { $($variant:pat => $body:expr),* $(,)? }) => {
        match $slot {
            $($variant => $body,)*
            _ => None,
        }
    };
}

/// Route slot_deps dispatch. Returns `0` for unmatched slots.
///
/// # Example
/// ```ignore
/// fn slot_deps(slot: u8) -> u16 {
///     kasane_plugin_sdk::route_slot_deps!(slot, {
///         slot::BUFFER_LEFT => dirty::BUFFER,
///     })
/// }
/// ```
#[macro_export]
macro_rules! route_slot_deps {
    ($slot:expr, { $($variant:pat => $deps:expr),* $(,)? }) => {
        match $slot {
            $($variant => $deps,)*
            _ => 0,
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_constants_match_count() {
        assert_eq!(slot::COUNT, 8);
        assert_eq!(slot::OVERLAY, 7);
    }

    #[test]
    fn dirty_flags_all_covers_all_bits() {
        assert_eq!(
            dirty::ALL,
            dirty::BUFFER | dirty::STATUS | dirty::MENU | dirty::INFO | dirty::OPTIONS
        );
    }

    #[test]
    fn dirty_flags_menu_is_composite() {
        assert_eq!(dirty::MENU, dirty::MENU_STRUCTURE | dirty::MENU_SELECTION);
    }

    #[test]
    fn dirty_all_matches_bitflags() {
        // Ensure our constants match kasane-core's DirtyFlags::ALL = 0x3F
        assert_eq!(dirty::ALL, 0x3F);
    }

    #[test]
    fn modifier_constants_match() {
        assert_eq!(modifiers::CTRL, 0x01);
        assert_eq!(modifiers::ALT, 0x02);
        assert_eq!(modifiers::SHIFT, 0x04);
    }
}
