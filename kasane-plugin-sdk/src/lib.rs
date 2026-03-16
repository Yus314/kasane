//! SDK for writing Kasane WASM plugins.
//!
//! Provides constants, helper macros, and the WIT interface definition
//! for building Kasane plugins targeting `wasm32-wasip2`.
//!
//! # Quick Start
//!
//! ```ignore
//! use kasane_plugin_sdk::plugin;
//!
//! kasane_plugin_sdk::generate!();
//!
//! use exports::kasane::plugin::plugin_api::Guest;
//! use kasane::plugin::types::*;
//! use kasane::plugin::host_state;
//! use kasane_plugin_sdk::dirty;
//!
//! struct MyPlugin;
//!
//! #[plugin]
//! impl Guest for MyPlugin {
//!     fn get_id() -> String { "my_plugin".into() }
//!
//!     fn contribute_to(region: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
//!         kasane_plugin_sdk::route_slot_ids!(region, {
//!             BUFFER_LEFT => {
//!                 // ... build elements via element_builder ...
//!                 None
//!             },
//!         })
//!     }
//!
//!     fn contribute_deps(region: SlotId) -> u16 {
//!         kasane_plugin_sdk::route_slot_id_deps!(region, {
//!             BUFFER_LEFT => dirty::BUFFER,
//!         })
//!     }
//!
//!     fn state_hash() -> u64 { 0 }
//! }
//!
//! export!(MyPlugin);
//! ```
//!
//! All `Guest` methods not listed in the `impl` block are automatically filled
//! with SDK defaults by the `#[plugin]` attribute macro.
//!
//! # Legacy Quick Start (without `#[plugin]`)
//!
//! The `#[plugin]` macro is recommended. If you prefer explicit control,
//! you can still use individual `default_*!()` macros:
//!
//! ```ignore
//! // Cargo.toml:
//! // [dependencies]
//! // kasane-plugin-sdk = "0.1"
//!
//! // src/lib.rs:
//! kasane_plugin_sdk::generate!();
//!
//! use exports::kasane::plugin::plugin_api::Guest;
//! use kasane::plugin::types::*;
//! use kasane::plugin::{host_state, element_builder};
//! use kasane_plugin_sdk::{dirty, slot};
//!
//! struct MyPlugin;
//!
//! impl Guest for MyPlugin {
//!     fn get_id() -> String { "my_plugin".into() }
//!
//!     fn contribute_to(region: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
//!         kasane_plugin_sdk::route_slot_ids!(region, {
//!             BUFFER_LEFT => {
//!                 // ... build elements via element_builder ...
//!                 None
//!             },
//!         })
//!     }
//!
//!     fn contribute_deps(region: SlotId) -> u16 {
//!         kasane_plugin_sdk::route_slot_id_deps!(region, {
//!             BUFFER_LEFT => dirty::BUFFER,
//!         })
//!     }
//!
//!     kasane_plugin_sdk::default_lifecycle!();
//!     kasane_plugin_sdk::default_cache!();
//!     kasane_plugin_sdk::default_input!();
//!     kasane_plugin_sdk::default_surfaces!();
//!     kasane_plugin_sdk::default_render_surface!();
//!     kasane_plugin_sdk::default_handle_surface_event!();
//!     kasane_plugin_sdk::default_handle_surface_state_changed!();
//!     // Old WIT stubs (required by interface, not called by host)
//!     kasane_plugin_sdk::default_contribute!();
//!     kasane_plugin_sdk::default_line!();
//!     kasane_plugin_sdk::default_overlay!();
//!     kasane_plugin_sdk::default_decorate!();
//!     kasane_plugin_sdk::default_replace!();
//!     kasane_plugin_sdk::default_decorator_priority!();
//!     kasane_plugin_sdk::default_named_slot!();
//!     // New API defaults
//!     kasane_plugin_sdk::default_menu_transform!();
//!     kasane_plugin_sdk::default_transform!();
//!     kasane_plugin_sdk::default_transform_priority!();
//!     kasane_plugin_sdk::default_annotate!();
//!     kasane_plugin_sdk::default_overlay_v2!();
//!     kasane_plugin_sdk::default_transform_deps!();
//!     kasane_plugin_sdk::default_annotate_deps!();
//!     kasane_plugin_sdk::default_cursor_style!();
//!     kasane_plugin_sdk::default_update!();
//!     kasane_plugin_sdk::default_capabilities!();
//! }
//!
//! export!(MyPlugin);
//! ```

/// Attribute macro that fills in default implementations for all
/// unimplemented `Guest` trait methods.
///
/// See the [module-level documentation](crate) for usage.
pub use kasane_plugin_sdk_macros::kasane_wasm_plugin as plugin;

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

/// Well-known slot names matching `kasane_core::plugin::SlotId`.
pub mod slot_name {
    pub const BUFFER_LEFT: &str = "kasane.buffer.left";
    pub const BUFFER_RIGHT: &str = "kasane.buffer.right";
    pub const ABOVE_BUFFER: &str = "kasane.buffer.above";
    pub const BELOW_BUFFER: &str = "kasane.buffer.below";
    pub const ABOVE_STATUS: &str = "kasane.status.above";
    pub const STATUS_LEFT: &str = "kasane.status.left";
    pub const STATUS_RIGHT: &str = "kasane.status.right";
    pub const OVERLAY: &str = "kasane.overlay";
}

/// DirtyFlags bit values matching `kasane_core::state::DirtyFlags`.
pub mod dirty {
    pub const BUFFER_CONTENT: u16 = 1 << 0;
    pub const STATUS: u16 = 1 << 1;
    pub const MENU_STRUCTURE: u16 = 1 << 2;
    pub const MENU_SELECTION: u16 = 1 << 3;
    pub const INFO: u16 = 1 << 4;
    pub const OPTIONS: u16 = 1 << 5;
    pub const BUFFER_CURSOR: u16 = 1 << 6;
    pub const SESSION: u16 = 1 << 8;
    /// Composite: any buffer-related change (content or cursor).
    pub const BUFFER: u16 = BUFFER_CONTENT | BUFFER_CURSOR;
    pub const MENU: u16 = MENU_STRUCTURE | MENU_SELECTION;
    pub const ALL: u16 = BUFFER | STATUS | MENU | INFO | OPTIONS | SESSION;
}

/// WASI capability identifiers matching the WIT `capability` enum ordinals.
pub mod capability {
    pub const FILESYSTEM: u8 = 0;
    pub const ENVIRONMENT: u8 = 1;
    pub const MONOTONIC_CLOCK: u8 = 2;
    pub const PROCESS: u8 = 3;
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
/// Two forms:
/// - `kasane_plugin_sdk::generate!()` — uses embedded WIT (recommended, works with crates.io)
/// - `kasane_plugin_sdk::generate!("path/to/wit")` — uses file path (monorepo dev)
///
/// # Example
/// ```ignore
/// kasane_plugin_sdk::generate!();
/// ```
///
/// The generated modules:
/// - `exports::kasane::plugin::plugin_api::Guest` — trait to implement
/// - `kasane::plugin::host_state` — host state query functions
/// - `kasane::plugin::element_builder` — element construction functions
/// - `kasane::plugin::types::*` — shared types (Face, Color, etc.)
///
/// Note: Guest crates must also depend on `wit-bindgen` directly, since
/// `wit_bindgen::generate!` generates code referencing `wit_bindgen` runtime types.
pub use kasane_plugin_sdk_macros::kasane_generate as generate;

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

/// Default hosted surface preflight stub (returns no surfaces).
#[macro_export]
macro_rules! default_surfaces {
    () => {
        fn surfaces() -> Vec<SurfaceDescriptor> {
            vec![]
        }
    };
}

/// Default hosted surface render stub (returns no surface content).
#[macro_export]
macro_rules! default_render_surface {
    () => {
        fn render_surface(_surface_key: String, _ctx: SurfaceViewContext) -> Option<ElementHandle> {
            None
        }
    };
}

/// Default hosted surface event stub (returns no commands).
#[macro_export]
macro_rules! default_handle_surface_event {
    () => {
        fn handle_surface_event(
            _surface_key: String,
            _event: SurfaceEvent,
            _ctx: SurfaceEventContext,
        ) -> Vec<Command> {
            vec![]
        }
    };
}

/// Default hosted surface state-change stub (returns no commands).
#[macro_export]
macro_rules! default_handle_surface_state_changed {
    () => {
        fn handle_surface_state_changed(_surface_key: String, _dirty_flags: u16) -> Vec<Command> {
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

/// Default contribute-to stub (returns None for all regions).
#[macro_export]
macro_rules! default_contribute_to {
    () => {
        fn contribute_to(_region: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
            None
        }
    };
}

/// Default transform-element stub (passes through element unchanged).
#[macro_export]
macro_rules! default_transform {
    () => {
        fn transform_element(
            _target: TransformTarget,
            element: ElementHandle,
            _ctx: TransformContext,
        ) -> ElementHandle {
            element
        }
    };
}

/// Default transform-priority stub (returns 0).
#[macro_export]
macro_rules! default_transform_priority {
    () => {
        fn transform_priority() -> i16 {
            0
        }
    };
}

/// Default annotate-line stub (returns None).
#[macro_export]
macro_rules! default_annotate {
    () => {
        fn annotate_line(_line: u32, _ctx: AnnotateContext) -> Option<LineAnnotation> {
            None
        }
    };
}

/// Default contribute-overlay-v2 stub (returns None).
#[macro_export]
macro_rules! default_overlay_v2 {
    () => {
        fn contribute_overlay_v2(_ctx: OverlayContext) -> Option<OverlayContribution> {
            None
        }
    };
}

/// Default contribute-deps stub (returns 0).
#[macro_export]
macro_rules! default_contribute_deps {
    () => {
        fn contribute_deps(_region: SlotId) -> u16 {
            0
        }
    };
}

/// Build a first-class slot identifier for `contribute_to()` / `contribute_deps()`.
///
/// # Example
/// ```ignore
/// let left = kasane_plugin_sdk::slot_id!(BUFFER_LEFT);
/// let custom = kasane_plugin_sdk::slot_id!(named("myplugin.sidebar.top"));
/// ```
#[macro_export]
macro_rules! slot_id {
    (BUFFER_LEFT) => {
        SlotId::WellKnown(WellKnownSlot::BufferLeft)
    };
    (BUFFER_RIGHT) => {
        SlotId::WellKnown(WellKnownSlot::BufferRight)
    };
    (ABOVE_BUFFER) => {
        SlotId::WellKnown(WellKnownSlot::AboveBuffer)
    };
    (BELOW_BUFFER) => {
        SlotId::WellKnown(WellKnownSlot::BelowBuffer)
    };
    (ABOVE_STATUS) => {
        SlotId::WellKnown(WellKnownSlot::AboveStatus)
    };
    (STATUS_LEFT) => {
        SlotId::WellKnown(WellKnownSlot::StatusLeft)
    };
    (STATUS_RIGHT) => {
        SlotId::WellKnown(WellKnownSlot::StatusRight)
    };
    (OVERLAY) => {
        SlotId::WellKnown(WellKnownSlot::Overlay)
    };
    (named($name:expr)) => {
        SlotId::Named(($name).into())
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __route_slot_ids_impl {
    ($slot:expr, { }) => {
        None
    };
    ($slot:expr, { named($name:expr) => $body:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::Named(name) if name == $name => $body,
            _ => $crate::__route_slot_ids_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { BUFFER_LEFT => $body:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::BufferLeft) => $body,
            _ => $crate::__route_slot_ids_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { BUFFER_RIGHT => $body:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::BufferRight) => $body,
            _ => $crate::__route_slot_ids_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { ABOVE_BUFFER => $body:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::AboveBuffer) => $body,
            _ => $crate::__route_slot_ids_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { BELOW_BUFFER => $body:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::BelowBuffer) => $body,
            _ => $crate::__route_slot_ids_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { ABOVE_STATUS => $body:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::AboveStatus) => $body,
            _ => $crate::__route_slot_ids_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { STATUS_LEFT => $body:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::StatusLeft) => $body,
            _ => $crate::__route_slot_ids_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { STATUS_RIGHT => $body:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::StatusRight) => $body,
            _ => $crate::__route_slot_ids_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { OVERLAY => $body:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::Overlay) => $body,
            _ => $crate::__route_slot_ids_impl!($slot, { $($rest)* }),
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __route_slot_id_deps_impl {
    ($slot:expr, { }) => {
        0
    };
    ($slot:expr, { named($name:expr) => $deps:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::Named(name) if name == $name => $deps,
            _ => $crate::__route_slot_id_deps_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { BUFFER_LEFT => $deps:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::BufferLeft) => $deps,
            _ => $crate::__route_slot_id_deps_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { BUFFER_RIGHT => $deps:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::BufferRight) => $deps,
            _ => $crate::__route_slot_id_deps_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { ABOVE_BUFFER => $deps:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::AboveBuffer) => $deps,
            _ => $crate::__route_slot_id_deps_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { BELOW_BUFFER => $deps:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::BelowBuffer) => $deps,
            _ => $crate::__route_slot_id_deps_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { ABOVE_STATUS => $deps:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::AboveStatus) => $deps,
            _ => $crate::__route_slot_id_deps_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { STATUS_LEFT => $deps:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::StatusLeft) => $deps,
            _ => $crate::__route_slot_id_deps_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { STATUS_RIGHT => $deps:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::StatusRight) => $deps,
            _ => $crate::__route_slot_id_deps_impl!($slot, { $($rest)* }),
        }
    };
    ($slot:expr, { OVERLAY => $deps:expr, $($rest:tt)* }) => {
        match $slot {
            SlotId::WellKnown(WellKnownSlot::Overlay) => $deps,
            _ => $crate::__route_slot_id_deps_impl!($slot, { $($rest)* }),
        }
    };
}

/// Default transform-deps stub (returns ALL).
#[macro_export]
macro_rules! default_transform_deps {
    () => {
        fn transform_deps(_target: TransformTarget) -> u16 {
            $crate::dirty::ALL
        }
    };
}

/// Default annotate-deps stub (returns ALL).
#[macro_export]
macro_rules! default_annotate_deps {
    () => {
        fn annotate_deps() -> u16 {
            $crate::dirty::ALL
        }
    };
}

/// Default requested-capabilities stub (returns empty list = no WASI capabilities).
#[macro_export]
macro_rules! default_capabilities {
    () => {
        fn requested_capabilities() -> Vec<Capability> {
            vec![]
        }
    };
}

/// Default on-io-event stub (returns empty command list).
#[macro_export]
macro_rules! default_io_event {
    () => {
        fn on_io_event(_event: IoEvent) -> Vec<Command> {
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

/// Route first-class slot-id dispatch. Returns `None` for unmatched slots.
///
/// # Example
/// ```ignore
/// fn contribute_to(slot: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
///     kasane_plugin_sdk::route_slot_ids!(slot, {
///         BUFFER_LEFT => {
///             Some(contribution)
///         },
///         named("myplugin.sidebar.top") => None,
///     })
/// }
/// ```
#[macro_export]
macro_rules! route_slot_ids {
    ($slot:expr, { $($rest:tt)* }) => {{
        let __slot = &$slot;
        $crate::__route_slot_ids_impl!(__slot, { $($rest)* })
    }};
}

/// Route first-class slot-id deps dispatch. Returns `0` for unmatched slots.
#[macro_export]
macro_rules! route_slot_id_deps {
    ($slot:expr, { $($rest:tt)* }) => {{
        let __slot = &$slot;
        $crate::__route_slot_id_deps_impl!(__slot, { $($rest)* })
    }};
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
            dirty::BUFFER | dirty::STATUS | dirty::MENU | dirty::INFO | dirty::OPTIONS | dirty::SESSION
        );
    }

    #[test]
    fn dirty_flags_menu_is_composite() {
        assert_eq!(dirty::MENU, dirty::MENU_STRUCTURE | dirty::MENU_SELECTION);
    }

    #[test]
    fn dirty_all_matches_bitflags() {
        // SDK's ALL intentionally excludes PLUGIN_STATE (bit 7). Core ALL = 0xFF, SDK ALL = 0x17F.
        assert_eq!(dirty::ALL, 0x17F);
    }

    #[test]
    fn test_session_constant() {
        assert_eq!(dirty::SESSION, 0x100);
    }

    #[test]
    fn modifier_constants_match() {
        assert_eq!(modifiers::CTRL, 0x01);
        assert_eq!(modifiers::ALT, 0x02);
        assert_eq!(modifiers::SHIFT, 0x04);
    }
}
