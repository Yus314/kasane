//! SDK for writing Kasane WASM plugins.
//!
//! Provides constants, helper macros, and the WIT interface definition
//! for building Kasane plugins targeting `wasm32-wasip2`.
//!
//! # Quick Start
//!
//! The simplest plugin is 3 lines with [`define_plugin!`]:
//!
//! ```ignore
//! kasane_plugin_sdk::define_plugin! {
//!     id: "my_plugin",
//!     slots {
//!         STATUS_RIGHT => plain(" Hello! "),
//!     },
//! }
//! ```
//!
//! `define_plugin!` combines WIT bindings, state declaration, `#[plugin]`,
//! and `export!()` into a single macro. It auto-imports `dirty`, `modifiers`,
//! `keys`, and `attributes` modules, plus SDK helpers like `plain()`,
//! `colored()`, `is_ctrl()`, `status_badge()`, `redraw()`, `paste_clipboard()`,
//! and `hex()`.
//!
//! ## With State
//!
//! Use `#[bind(expr, on: flags)]` to auto-sync state from host:
//!
//! ```ignore
//! kasane_plugin_sdk::define_plugin! {
//!     id: "sel_badge",
//!
//!     state {
//!         #[bind(host_state::get_cursor_count(), on: dirty::BUFFER)]
//!         cursor_count: u32 = 0,
//!     },
//!
//!     slots {
//!         STATUS_RIGHT(dirty::BUFFER) => |_ctx| {
//!             status_badge(state.cursor_count > 1, &format!(" {} sel ", state.cursor_count))
//!         },
//!     },
//! }
//! ```
//!
//! Inside `slots` closures, `state.field` is available directly (read-only).
//! In `handle_key`, `overlay`, `on_io_event_effects`, etc., `state` is mutable and
//! `bump_generation()` is called automatically when the guard drops.
//!
//! For backend-independent physical chrome, implement `Guest::render_ornaments`
//! and return an `OrnamentBatch` with emphasis, cursor, or surface proposals.
//!
//! # Explicit Pattern
//!
//! For full control over state management, use `generate!()` + `#[plugin]`
//! + `export!()` separately:
//!
//! ```ignore
//! kasane_plugin_sdk::generate!();
//!
//! use kasane_plugin_sdk::{dirty, plugin};
//!
//! struct MyPlugin;
//!
//! #[plugin]
//! impl Guest for MyPlugin {
//!     fn get_id() -> String { "my_plugin".into() }
//!
//!     kasane_plugin_sdk::slots! {
//!         BUFFER_LEFT(dirty::BUFFER) => |_ctx| {
//!             Some(auto_contribution(text("★", default_style())))
//!         },
//!     }
//! }
//!
//! export!(MyPlugin);
//! ```
//!
//! `generate!()` emits WIT bindings and auto-imports common types (`Guest`,
//! `host_state`, `element_builder`, `types::*`) plus helper functions
//! (`default_style()`, `rgb()`, `style_bg()`, `plain()`, `colored()`,
//! `flex_row()`, `flex_column()`, `grid()`, `scrollable()`, `flex_entry()`,
//! `empty()`, etc.).
//!
//! `#[plugin]` fills in default implementations for all `Guest` methods
//! you don't write.

/// Attribute macro that fills in default implementations for all
/// unimplemented `Guest` trait methods.
///
/// See the [module-level documentation](crate) for usage.
pub use kasane_plugin_sdk_macros::kasane_wasm_plugin as plugin;

/// All-in-one plugin definition macro.
///
/// Combines `generate!()`, `state!`, `#[plugin]`, and `export!()` into a
/// single declaration. See the proc macro documentation for full syntax.
pub use kasane_plugin_sdk_macros::kasane_define_plugin as define_plugin;

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
///
/// Each flag indicates what part of the editor state changed. Use these
/// in `on_state_changed_effects()` to selectively update cached data.
pub mod dirty {
    /// Buffer line contents changed (Kakoune `draw` command).
    pub const BUFFER_CONTENT: u16 = 1 << 0;
    /// Status bar changed (Kakoune `draw_status` command).
    pub const STATUS: u16 = 1 << 1;
    /// Menu items added or removed (`menu_show` / `menu_hide`).
    pub const MENU_STRUCTURE: u16 = 1 << 2;
    /// Menu selection index changed (`menu_select`).
    pub const MENU_SELECTION: u16 = 1 << 3;
    /// Info popup changed (`info_show` / `info_hide`).
    pub const INFO: u16 = 1 << 4;
    /// UI options changed (`set_ui_options`).
    pub const OPTIONS: u16 = 1 << 5;
    /// Cursor position or mode changed.
    pub const BUFFER_CURSOR: u16 = 1 << 6;
    /// Another plugin's state changed (bit 7).
    ///
    /// Excluded from `ALL` because inter-plugin observation is opt-in:
    /// most plugins only care about editor state, not sibling plugins.
    pub const PLUGIN_STATE: u16 = 1 << 7;
    /// Session metadata changed (session added/removed/switched).
    pub const SESSION: u16 = 1 << 8;
    /// Plugin settings changed (typed per-plugin configuration).
    pub const SETTINGS: u16 = 1 << 9;
    /// Composite: any buffer-related change (content or cursor).
    pub const BUFFER: u16 = BUFFER_CONTENT | BUFFER_CURSOR;
    /// Composite: any menu-related change (structure or selection).
    pub const MENU: u16 = MENU_STRUCTURE | MENU_SELECTION;
    /// All flags combined (excludes PLUGIN_STATE — opt-in only).
    pub const ALL: u16 = BUFFER | STATUS | MENU | INFO | OPTIONS | SESSION | SETTINGS;
}

/// WASI capability identifiers matching the WIT `capability` enum ordinals.
pub mod capability {
    pub const FILESYSTEM: u8 = 0;
    pub const ENVIRONMENT: u8 = 1;
    pub const MONOTONIC_CLOCK: u8 = 2;
    pub const PROCESS: u8 = 3;
}

/// Kasane host authority identifiers matching the WIT `plugin-authority` enum ordinals.
pub mod authority {
    pub const DYNAMIC_SURFACE: u8 = 0;
    pub const PTY_PROCESS: u8 = 1;
    pub const WORKSPACE_MANAGEMENT: u8 = 2;
}

/// Helpers for ChannelValue (postcard) serialization at the WASM boundary.
///
/// WASM plugins work with the WIT-generated `ChannelValue { data, type_hint }` struct.
/// This module provides serialization/deserialization of the raw bytes.
pub mod channel {
    use serde::{Serialize, de::DeserializeOwned};

    /// Serialize a value into `(data, type_hint)` for a WIT `channel-value`.
    pub fn serialize<T: Serialize>(value: &T) -> (Vec<u8>, String) {
        let data = postcard::to_allocvec(value).expect("ChannelValue serialize failed");
        let type_hint = std::any::type_name::<T>().to_string();
        (data, type_hint)
    }

    /// Deserialize from WIT `channel-value` data bytes.
    pub fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Option<T> {
        postcard::from_bytes(data).ok()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn round_trip_u32() {
            let (data, hint) = serialize(&42u32);
            assert!(hint.contains("u32"));
            assert_eq!(deserialize::<u32>(&data), Some(42));
        }

        #[test]
        fn round_trip_string() {
            let (data, _hint) = serialize(&"hello".to_string());
            assert_eq!(deserialize::<String>(&data), Some("hello".to_string()));
        }

        #[test]
        fn round_trip_vec() {
            let (data, _hint) = serialize(&vec![1u32, 2, 3]);
            assert_eq!(deserialize::<Vec<u32>>(&data), Some(vec![1, 2, 3]));
        }

        #[test]
        fn round_trip_struct() {
            #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
            struct Point { x: i32, y: i32 }
            let (data, _hint) = serialize(&Point { x: 10, y: 20 });
            assert_eq!(deserialize::<Point>(&data), Some(Point { x: 10, y: 20 }));
        }

        #[test]
        fn deserialize_wrong_type_returns_none() {
            let (data, _hint) = serialize(&42u32);
            assert_eq!(deserialize::<String>(&data), None);
        }
    }
}

/// Predicate builder macros for conditional transform patches.
///
/// These macros generate RPN-encoded `PredicateOp` sequences for use in
/// `when` patches. The `PredicateOp` type is generated by `wit_bindgen`
/// and must be in scope at the call site (i.e., inside a plugin crate
/// after `generate!()` or `define_plugin!`).
///
/// # Example
///
/// ```ignore
/// use kasane_plugin_sdk::{pred_has_focus, pred_not};
/// let pred = pred_not!(pred_has_focus!());
/// ```
#[macro_export]
macro_rules! pred_has_focus {
    () => { vec![PredicateOp::HasFocus] };
}

#[macro_export]
macro_rules! pred_surface_is {
    ($id:expr) => { vec![PredicateOp::SurfaceIs($id)] };
}

#[macro_export]
macro_rules! pred_line_range {
    ($start:expr, $end:expr) => {
        vec![PredicateOp::LineRange(LineRangePredicate { start: $start, end: $end })]
    };
}

#[macro_export]
macro_rules! pred_not {
    ($inner:expr) => {{ let mut ops = $inner; ops.push(PredicateOp::NotOp); ops }};
}

#[macro_export]
macro_rules! pred_and {
    ($a:expr, $b:expr) => {{ let mut ops = $a; ops.extend($b); ops.push(PredicateOp::AndOp); ops }};
}

#[macro_export]
macro_rules! pred_or {
    ($a:expr, $b:expr) => {{ let mut ops = $a; ops.extend($b); ops.push(PredicateOp::OrOp); ops }};
}

/// Modifier key bitflags matching `kasane_core::input::Modifiers`.
pub mod modifiers {
    pub const CTRL: u8 = 0b0000_0001;
    pub const ALT: u8 = 0b0000_0010;
    pub const SHIFT: u8 = 0b0000_0100;
}

/// Key escaping helpers for building Kakoune keystroke sequences.
///
/// Kakoune's `SendKeys` command accepts a list of individual key strings.
/// Special characters must be escaped (e.g., space → `<space>`, `<` → `<lt>`).
pub mod keys {
    /// Push each character of `text` as an escaped key string.
    ///
    /// Special characters are converted to their Kakoune key names:
    /// - space → `<space>`
    /// - `<` → `<lt>`, `>` → `<gt>`
    /// - `-` → `<minus>`, `%` → `<percent>`
    pub fn push_literal(keys: &mut Vec<String>, text: &str) {
        for ch in text.chars() {
            match ch {
                ' ' => keys.push("<space>".into()),
                '<' => keys.push("<lt>".into()),
                '>' => keys.push("<gt>".into()),
                '-' => keys.push("<minus>".into()),
                '%' => keys.push("<percent>".into()),
                c => keys.push(c.to_string()),
            }
        }
    }

    /// Build a key sequence that escapes to normal mode, runs a Kakoune command,
    /// and presses return: `<esc>:cmd<ret>`.
    pub fn command(cmd: &str) -> Vec<String> {
        let mut keys = vec!["<esc>".to_string(), ":".to_string()];
        push_literal(&mut keys, cmd);
        keys.push("<ret>".to_string());
        keys
    }
}

/// Legacy attribute bitflags. The post-resolve `Style` record exposes these
/// as separate fields (`font_weight`, `font_slant`, `underline`,
/// `strikethrough`, `blink`, `reverse`, `dim`); use the bitset only with
/// the SDK-provided `style_full(fg, bg, underline_color, attrs)` helper for
/// migration ergonomics.
pub mod attributes {
    pub const UNDERLINE: u16 = 1 << 0;
    pub const CURLY_UNDERLINE: u16 = 1 << 1;
    pub const DOUBLE_UNDERLINE: u16 = 1 << 2;
    pub const REVERSE: u16 = 1 << 3;
    pub const BLINK: u16 = 1 << 4;
    pub const BOLD: u16 = 1 << 5;
    pub const DIM: u16 = 1 << 6;
    pub const ITALIC: u16 = 1 << 7;
    pub const STRIKETHROUGH: u16 = 1 << 8;
}

/// Editor mode constants returned by `host_state::get_editor_mode()`.
pub mod editor_mode {
    pub const NORMAL: u8 = 0;
    pub const INSERT: u8 = 1;
    pub const REPLACE: u8 = 2;
    pub const PROMPT: u8 = 3;
    pub const UNKNOWN: u8 = 255;
}

/// `StyleMerge` mode constants for `CellDecoration`.
pub mod style_merge {
    /// Completely replace the existing cell style.
    pub const REPLACE: u8 = 0;
    /// Overlay non-default fields onto the existing style.
    pub const OVERLAY: u8 = 1;
    /// Only apply the background brush from the decoration style.
    pub const BACKGROUND: u8 = 2;
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
/// - `kasane::plugin::types::*` — shared types (Style, Brush, etc.)
///
/// Note: Guest crates must also depend on `wit-bindgen` directly, since
/// `wit_bindgen::generate!` generates code referencing `wit_bindgen` runtime types.
pub use kasane_plugin_sdk_macros::kasane_generate as generate;

/// Build a first-class slot identifier for `contribute_to()`.
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

/// Declare thread-local plugin state with a generation counter.
///
/// Generates a struct with the specified fields plus a `generation: u64` field,
/// a `Default` implementation using the provided default values,
/// a `bump_generation()` method, and a `thread_local!` static `STATE`.
///
/// # Example
///
/// ```ignore
/// kasane_plugin_sdk::state! {
///     struct PluginState {
///         cursor_count: u32 = 0,
///         active: bool = false,
///     }
/// }
/// // Generates:
/// // - struct PluginState { cursor_count: u32, active: bool, generation: u64 }
/// // - impl Default for PluginState { ... }  (with cursor_count: 0, active: false, generation: 0)
/// // - impl PluginState { fn bump_generation(&mut self) { ... } }
/// // - thread_local! { static STATE: RefCell<PluginState> = ... }
/// ```
///
/// Access the state via `STATE.with(|s| { let state = s.borrow(); ... })`.
/// The `generation` field is for use in `state_hash()` — call `bump_generation()`
/// whenever the plugin's visible output would change.
#[macro_export]
macro_rules! state {
    (
        struct $name:ident {
            $( $field:ident : $ty:ty = $default:expr ),* $(,)?
        }
    ) => {
        #[derive(Debug)]
        struct $name {
            $( $field: $ty, )*
            generation: u64,
        }

        impl Default for $name {
            fn default() -> Self {
                Self {
                    $( $field: $default, )*
                    generation: 0,
                }
            }
        }

        impl $name {
            fn bump_generation(&mut self) {
                self.generation = self.generation.wrapping_add(1);
            }
        }

        ::std::thread_local! {
            static STATE: ::std::cell::RefCell<$name> = ::std::cell::RefCell::new(<$name>::default());
        }

        /// Auto-generated state hash based on generation counter.
        /// Override by implementing `state_hash()` manually in Guest impl.
        #[doc(hidden)]
        #[allow(dead_code)]
        fn __kasane_auto_state_hash() -> u64 {
            STATE.with(|s| s.borrow().generation)
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

/// Type-safe interactive element ID encoding/decoding.
///
/// Generates an enum with `encode()` and `decode()` methods that pack variant +
/// field data into a single `u32` interactive ID.
///
/// # Namespaced form (recommended)
///
/// With `PluginTag`-based namespace isolation, `base` defaults to 0 and `stride`
/// is auto-calculated from field types. This is the recommended form:
///
/// ```ignore
/// kasane_plugin_sdk::interactive_id! {
///     enum PickerId {
///         Swatch,
///         Channel { idx: u8, ch: u8, down: bool },
///         Close,
///     }
/// }
/// ```
///
/// # Legacy form (explicit base + stride)
///
/// ```ignore
/// kasane_plugin_sdk::interactive_id! {
///     enum PickerId(base = 2000, stride = 6) {
///         Swatch,
///         Channel { idx: u8, ch: u8, down: bool },
///         Close,
///     }
/// }
/// ```
///
/// - `base`: starting ID value (default: 0)
/// - `stride`: multiplier per variant (default: auto-calculated from field widths)
/// - Fieldless variants encode as `base + tag * stride`
/// - Field variants pack fields in declaration order using bit-level little-endian:
///   `u8` → 8 bits, `bool` → 1 bit, `u16` → 16 bits
#[macro_export]
macro_rules! interactive_id {
    // Namespaced form: no base/stride — auto-calculate stride from field widths.
    (
        enum $name:ident {
            $( $variant:ident $( { $( $field:ident : $fty:tt ),* $(,)? } )? ),* $(,)?
        }
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum $name {
            $( $variant $( { $( $field: $fty ),* } )? ),*
        }

        impl $name {
            const __AUTO_STRIDE: u32 = {
                let mut max = 1u32;
                $({
                    let w = 0u32 $($(+ $crate::__iid_width!($fty))*)?;
                    let s = 1u32 << w;
                    if s > max { max = s; }
                })*
                max
            };

            #[allow(unused_assignments, unused_variables)]
            fn encode(&self) -> u32 {
                let mut __tag: u32 = 0;
                $(
                    if let $name::$variant $( { $( $field ),* } )? = *self {
                        let mut __packed: u32 = 0;
                        let mut __shift: u32 = 0;
                        $($(
                            __packed |= ($field as u32) << __shift;
                            __shift += $crate::__iid_width!($fty);
                        )*)?
                        return __tag * Self::__AUTO_STRIDE + __packed;
                    }
                    __tag += 1;
                )*
                unreachable!()
            }

            #[allow(unused_assignments, unused_variables, unused_mut)]
            fn decode(id: u32) -> Option<Self> {
                let __tag = id / Self::__AUTO_STRIDE;
                let __rem = id % Self::__AUTO_STRIDE;

                let mut __expected_tag: u32 = 0;
                $(
                    if __tag == __expected_tag {
                        let mut __v: u32 = __rem;
                        $($(
                            let $field = $crate::__iid_dec!(__v, $fty);
                        )*)?
                        return Some($name::$variant $( { $( $field ),* } )? );
                    }
                    __expected_tag += 1;
                )*
                None
            }
        }
    };
    // Legacy form: explicit base + stride.
    (
        enum $name:ident (base = $base:expr, stride = $stride:expr) {
            $( $variant:ident $( { $( $field:ident : $fty:tt ),* $(,)? } )? ),* $(,)?
        }
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum $name {
            $( $variant $( { $( $field: $fty ),* } )? ),*
        }

        impl $name {
            #[allow(unused_assignments, unused_variables)]
            fn encode(&self) -> u32 {
                let mut __tag: u32 = 0;
                $(
                    if let $name::$variant $( { $( $field ),* } )? = *self {
                        let mut __packed: u32 = 0;
                        let mut __shift: u32 = 0;
                        $($(
                            __packed |= ($field as u32) << __shift;
                            __shift += $crate::__iid_width!($fty);
                        )*)?
                        return ($base) + __tag * ($stride) + __packed;
                    }
                    __tag += 1;
                )*
                unreachable!()
            }

            #[allow(unused_assignments, unused_variables, unused_mut)]
            fn decode(id: u32) -> Option<Self> {
                if id < ($base) {
                    return None;
                }
                let __rel = id - ($base);
                let __tag = __rel / ($stride);
                let __rem = __rel % ($stride);

                let mut __expected_tag: u32 = 0;
                $(
                    if __tag == __expected_tag {
                        let mut __v: u32 = __rem;
                        $($(
                            let $field = $crate::__iid_dec!(__v, $fty);
                        )*)?
                        return Some($name::$variant $( { $( $field ),* } )? );
                    }
                    __expected_tag += 1;
                )*
                None
            }
        }
    };
}

/// Width in bits for each supported interactive_id field type.
#[doc(hidden)]
#[macro_export]
macro_rules! __iid_width {
    (u8) => {
        8
    };
    (bool) => {
        1
    };
    (u16) => {
        16
    };
}

/// Decode a field from a packed u32, consuming bits by shifting.
#[doc(hidden)]
#[macro_export]
macro_rules! __iid_dec {
    ($v:ident, bool) => {{
        let __r = ($v & 0x1) != 0;
        $v >>= 1;
        __r
    }};
    ($v:ident, u8) => {{
        let __r = ($v & 0xFF) as u8;
        $v >>= 8;
        __r
    }};
    ($v:ident, u16) => {{
        let __r = ($v & 0xFFFF) as u16;
        $v >>= 16;
        __r
    }};
}

/// Helpers for tracking async job results with generation-based stale detection.
///
/// Useful for plugins that spawn external processes and need to handle
/// out-of-order or stale results.
///
/// # Example
/// ```
/// use kasane_plugin_sdk::job::JobTracker;
///
/// let mut tracker = JobTracker::new(100);
/// let job1 = tracker.current_id();
/// assert_eq!(job1, 100);
///
/// // Advance to next generation (invalidates previous)
/// let job2 = tracker.advance();
/// assert_eq!(job2, 101);
/// assert!(!tracker.is_current(job1));
/// assert!(tracker.is_current(job2));
///
/// // Stale data is rejected
/// assert!(!tracker.append_stdout(job1, "old"));
/// assert!(tracker.append_stdout(job2, "new"));
/// assert_eq!(tracker.take_output(), "new");
/// ```
pub mod job {
    /// Tracks async job generations, automatically discarding stale results.
    #[derive(Debug)]
    pub struct JobTracker {
        base_id: u64,
        generation: u64,
        buffer: String,
    }

    impl JobTracker {
        /// Create a new tracker with the given base job ID.
        pub fn new(base_id: u64) -> Self {
            Self {
                base_id,
                generation: 0,
                buffer: String::new(),
            }
        }

        /// The current job ID (base + generation).
        pub fn current_id(&self) -> u64 {
            self.base_id + self.generation
        }

        /// Check if the given job ID matches the current generation.
        pub fn is_current(&self, job_id: u64) -> bool {
            job_id == self.current_id()
        }

        /// Advance to the next generation, clearing the output buffer.
        /// Returns the new current job ID.
        pub fn advance(&mut self) -> u64 {
            self.generation += 1;
            self.buffer.clear();
            self.current_id()
        }

        /// Append stdout data for the given job ID.
        /// Returns `false` (and discards data) if the job ID is stale.
        pub fn append_stdout(&mut self, job_id: u64, data: &str) -> bool {
            if !self.is_current(job_id) {
                return false;
            }
            self.buffer.push_str(data);
            true
        }

        /// Take the accumulated output, leaving the buffer empty.
        pub fn take_output(&mut self) -> String {
            std::mem::take(&mut self.buffer)
        }

        /// Iterate over lines in the accumulated output.
        pub fn lines(&self) -> impl Iterator<Item = &str> {
            self.buffer.lines()
        }
    }
}

/// Process pipeline helpers for plugins that spawn external commands.
///
/// Provides [`ProcessHandle`] for managing primary + fallback process patterns
/// (e.g. try `fd`, fall back to `find`), and [`ProcessStep`] / [`ProcessResult`]
/// for describing and inspecting process outcomes.
///
/// These types are pure Rust and do not depend on WIT bindings, so they can
/// be unit-tested without a WASM runtime.
pub mod process {
    /// Description of an external command to spawn.
    #[derive(Debug, Clone)]
    pub struct ProcessStep {
        pub program: String,
        pub args: Vec<String>,
    }

    /// Result returned by [`ProcessHandle::feed`].
    #[derive(Debug)]
    pub enum ProcessResult {
        /// More data expected — keep feeding events.
        Pending,
        /// Process completed successfully with accumulated stdout.
        Completed(Vec<u8>),
        /// Primary process failed — caller should spawn the fallback.
        TryFallback,
        /// Process failed with an error message.
        Failed(String),
        /// Event was for an unrecognized job id.
        Ignored,
    }

    /// Discriminated I/O event kind, borrowing data from the caller.
    pub enum IoEventKind<'a> {
        Stdout(&'a [u8]),
        Stderr(&'a [u8]),
        Exited(i32),
        SpawnFailed(&'a str),
    }

    /// Manages a primary process with an optional fallback.
    ///
    /// Accumulates stdout, detects success/failure, and signals when the
    /// caller should try the fallback command.
    #[derive(Debug)]
    pub struct ProcessHandle {
        primary_job_id: u64,
        fallback_job_id: Option<u64>,
        fallback_step: Option<ProcessStep>,
        buffer: Vec<u8>,
        using_fallback: bool,
    }

    impl ProcessHandle {
        /// Create a handle for a single primary process.
        pub fn new(job_id: u64) -> Self {
            ProcessHandle {
                primary_job_id: job_id,
                fallback_job_id: None,
                fallback_step: None,
                buffer: Vec::new(),
                using_fallback: false,
            }
        }

        /// Attach a fallback command to try if the primary fails.
        pub fn with_fallback(mut self, fallback_job_id: u64, step: ProcessStep) -> Self {
            self.fallback_job_id = Some(fallback_job_id);
            self.fallback_step = Some(step);
            self
        }

        /// The job id of the primary process.
        pub fn primary_job_id(&self) -> u64 {
            self.primary_job_id
        }

        /// The fallback step and its job id, if configured.
        pub fn fallback_info(&self) -> Option<(&ProcessStep, u64)> {
            match (&self.fallback_step, self.fallback_job_id) {
                (Some(step), Some(id)) => Some((step, id)),
                _ => None,
            }
        }

        /// Feed an I/O event and get back a result.
        ///
        /// - Returns `Ignored` if `job_id` doesn't match primary or fallback.
        /// - `Stdout` data is accumulated internally.
        /// - `Stderr` is silently ignored (returns `Pending`).
        /// - `Exited(0)` or non-empty buffer → `Completed`.
        /// - `Exited(non-zero)` with empty buffer → `TryFallback` (primary) or `Failed` (fallback).
        /// - `SpawnFailed` → `TryFallback` (primary) or `Failed` (fallback).
        pub fn feed(&mut self, job_id: u64, event: IoEventKind<'_>) -> ProcessResult {
            let is_primary = job_id == self.primary_job_id && !self.using_fallback;
            let is_fallback = self.fallback_job_id == Some(job_id) && self.using_fallback;

            if !is_primary && !is_fallback {
                return ProcessResult::Ignored;
            }

            match event {
                IoEventKind::Stdout(data) => {
                    self.buffer.extend_from_slice(data);
                    ProcessResult::Pending
                }
                IoEventKind::Stderr(_) => ProcessResult::Pending,
                IoEventKind::Exited(code) => {
                    if code == 0 || !self.buffer.is_empty() {
                        ProcessResult::Completed(std::mem::take(&mut self.buffer))
                    } else if is_primary {
                        self.using_fallback = true;
                        ProcessResult::TryFallback
                    } else {
                        ProcessResult::Failed(format!("process exited with code {code}"))
                    }
                }
                IoEventKind::SpawnFailed(msg) => {
                    if is_primary {
                        self.using_fallback = true;
                        ProcessResult::TryFallback
                    } else {
                        ProcessResult::Failed(msg.to_string())
                    }
                }
            }
        }

        /// Take the accumulated output buffer, leaving it empty.
        pub fn take_output(&mut self) -> Vec<u8> {
            std::mem::take(&mut self.buffer)
        }

        /// Reset the handle to its initial state (clears buffer and fallback flag).
        pub fn reset(&mut self) {
            self.buffer.clear();
            self.using_fallback = false;
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn primary_stdout_then_exit_zero() {
            let mut h = ProcessHandle::new(1);
            assert!(matches!(
                h.feed(1, IoEventKind::Stdout(b"hello")),
                ProcessResult::Pending
            ));
            match h.feed(1, IoEventKind::Exited(0)) {
                ProcessResult::Completed(data) => assert_eq!(data, b"hello"),
                other => panic!("expected Completed, got {other:?}"),
            }
        }

        #[test]
        fn primary_exit_nonzero_empty_tries_fallback() {
            let mut h = ProcessHandle::new(1).with_fallback(
                2,
                ProcessStep {
                    program: "find".into(),
                    args: vec![],
                },
            );
            match h.feed(1, IoEventKind::Exited(1)) {
                ProcessResult::TryFallback => {}
                other => panic!("expected TryFallback, got {other:?}"),
            }
        }

        #[test]
        fn primary_spawn_failed_tries_fallback() {
            let mut h = ProcessHandle::new(1).with_fallback(
                2,
                ProcessStep {
                    program: "find".into(),
                    args: vec![],
                },
            );
            match h.feed(1, IoEventKind::SpawnFailed("not found")) {
                ProcessResult::TryFallback => {}
                other => panic!("expected TryFallback, got {other:?}"),
            }
        }

        #[test]
        fn fallback_completes() {
            let mut h = ProcessHandle::new(1).with_fallback(
                2,
                ProcessStep {
                    program: "find".into(),
                    args: vec![],
                },
            );
            // Primary fails
            assert!(matches!(
                h.feed(1, IoEventKind::SpawnFailed("nope")),
                ProcessResult::TryFallback
            ));
            // Fallback produces output
            assert!(matches!(
                h.feed(2, IoEventKind::Stdout(b"file.txt\n")),
                ProcessResult::Pending
            ));
            match h.feed(2, IoEventKind::Exited(0)) {
                ProcessResult::Completed(data) => assert_eq!(data, b"file.txt\n"),
                other => panic!("expected Completed, got {other:?}"),
            }
        }

        #[test]
        fn fallback_fails() {
            let mut h = ProcessHandle::new(1).with_fallback(
                2,
                ProcessStep {
                    program: "find".into(),
                    args: vec![],
                },
            );
            assert!(matches!(
                h.feed(1, IoEventKind::SpawnFailed("nope")),
                ProcessResult::TryFallback
            ));
            match h.feed(2, IoEventKind::SpawnFailed("also not found")) {
                ProcessResult::Failed(msg) => assert!(msg.contains("also not found")),
                other => panic!("expected Failed, got {other:?}"),
            }
        }

        #[test]
        fn unknown_job_id_ignored() {
            let mut h = ProcessHandle::new(1);
            assert!(matches!(
                h.feed(99, IoEventKind::Stdout(b"x")),
                ProcessResult::Ignored
            ));
        }

        #[test]
        fn stderr_is_pending() {
            let mut h = ProcessHandle::new(1);
            assert!(matches!(
                h.feed(1, IoEventKind::Stderr(b"warn")),
                ProcessResult::Pending
            ));
        }

        #[test]
        fn primary_exit_nonzero_with_data_completes() {
            let mut h = ProcessHandle::new(1);
            assert!(matches!(
                h.feed(1, IoEventKind::Stdout(b"partial")),
                ProcessResult::Pending
            ));
            match h.feed(1, IoEventKind::Exited(1)) {
                ProcessResult::Completed(data) => assert_eq!(data, b"partial"),
                other => panic!("expected Completed, got {other:?}"),
            }
        }

        #[test]
        fn take_output_clears_buffer() {
            let mut h = ProcessHandle::new(1);
            h.feed(1, IoEventKind::Stdout(b"data"));
            let out = h.take_output();
            assert_eq!(out, b"data");
            assert!(h.take_output().is_empty());
        }

        #[test]
        fn reset_clears_state() {
            let mut h = ProcessHandle::new(1).with_fallback(
                2,
                ProcessStep {
                    program: "find".into(),
                    args: vec![],
                },
            );
            h.feed(1, IoEventKind::SpawnFailed("nope"));
            h.reset();
            // After reset, primary events should work again
            assert!(matches!(
                h.feed(1, IoEventKind::Stdout(b"ok")),
                ProcessResult::Pending
            ));
        }

        #[test]
        fn fallback_info_present() {
            let step = ProcessStep {
                program: "find".into(),
                args: vec![".".into()],
            };
            let h = ProcessHandle::new(1).with_fallback(2, step);
            let (s, id) = h.fallback_info().unwrap();
            assert_eq!(s.program, "find");
            assert_eq!(id, 2);
        }

        #[test]
        fn fallback_info_absent() {
            let h = ProcessHandle::new(1);
            assert!(h.fallback_info().is_none());
        }
    }
}

/// Unified slot contribution declaration.
///
/// Generates a `contribute_to` method from a single declaration block.
/// Use inside a `#[plugin] impl Guest` block.
///
/// # Example
/// ```ignore
/// #[plugin]
/// impl Guest for MyPlugin {
///     kasane_plugin_sdk::slots! {
///         STATUS_RIGHT => |_ctx| {
///             Some(auto_contribution(text("hello", default_style())))
///         },
///         named("my.slot") => |ctx| {
///             None
///         },
///     }
/// }
/// ```
#[macro_export]
macro_rules! slots {
    ( $( $slot_def:tt => |$ctx:ident| $body:block ),* $(,)? ) => {
        fn contribute_to(__region: SlotId, __ctx: ContributeContext) -> Option<Contribution> {
            $crate::__slots_contribute_impl!(__region, __ctx, $( $slot_def => |$ctx| $body ),*)
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __slots_contribute_impl {
    // terminal
    ($region:expr, $ctx_val:expr, ) => { None };
    // named slot
    ($region:expr, $ctx_val:expr, named($name:expr) => |$ctx:ident| $body:block $( , $($rest:tt)* )? ) => {
        match &$region {
            SlotId::Named(name) if name == $name => {
                let $ctx = &$ctx_val;
                $body
            }
            _ => $crate::__slots_contribute_impl!($region, $ctx_val, $( $($rest)* )? )
        }
    };
    // well-known slot — use tt munching for each variant
    ($region:expr, $ctx_val:expr, BUFFER_LEFT => |$ctx:ident| $body:block $( , $($rest:tt)* )? ) => {
        match &$region {
            SlotId::WellKnown(WellKnownSlot::BufferLeft) => { let $ctx = &$ctx_val; $body }
            _ => $crate::__slots_contribute_impl!($region, $ctx_val, $( $($rest)* )? )
        }
    };
    ($region:expr, $ctx_val:expr, BUFFER_RIGHT => |$ctx:ident| $body:block $( , $($rest:tt)* )? ) => {
        match &$region {
            SlotId::WellKnown(WellKnownSlot::BufferRight) => { let $ctx = &$ctx_val; $body }
            _ => $crate::__slots_contribute_impl!($region, $ctx_val, $( $($rest)* )? )
        }
    };
    ($region:expr, $ctx_val:expr, ABOVE_BUFFER => |$ctx:ident| $body:block $( , $($rest:tt)* )? ) => {
        match &$region {
            SlotId::WellKnown(WellKnownSlot::AboveBuffer) => { let $ctx = &$ctx_val; $body }
            _ => $crate::__slots_contribute_impl!($region, $ctx_val, $( $($rest)* )? )
        }
    };
    ($region:expr, $ctx_val:expr, BELOW_BUFFER => |$ctx:ident| $body:block $( , $($rest:tt)* )? ) => {
        match &$region {
            SlotId::WellKnown(WellKnownSlot::BelowBuffer) => { let $ctx = &$ctx_val; $body }
            _ => $crate::__slots_contribute_impl!($region, $ctx_val, $( $($rest)* )? )
        }
    };
    ($region:expr, $ctx_val:expr, ABOVE_STATUS => |$ctx:ident| $body:block $( , $($rest:tt)* )? ) => {
        match &$region {
            SlotId::WellKnown(WellKnownSlot::AboveStatus) => { let $ctx = &$ctx_val; $body }
            _ => $crate::__slots_contribute_impl!($region, $ctx_val, $( $($rest)* )? )
        }
    };
    ($region:expr, $ctx_val:expr, STATUS_LEFT => |$ctx:ident| $body:block $( , $($rest:tt)* )? ) => {
        match &$region {
            SlotId::WellKnown(WellKnownSlot::StatusLeft) => { let $ctx = &$ctx_val; $body }
            _ => $crate::__slots_contribute_impl!($region, $ctx_val, $( $($rest)* )? )
        }
    };
    ($region:expr, $ctx_val:expr, STATUS_RIGHT => |$ctx:ident| $body:block $( , $($rest:tt)* )? ) => {
        match &$region {
            SlotId::WellKnown(WellKnownSlot::StatusRight) => { let $ctx = &$ctx_val; $body }
            _ => $crate::__slots_contribute_impl!($region, $ctx_val, $( $($rest)* )? )
        }
    };
    ($region:expr, $ctx_val:expr, OVERLAY => |$ctx:ident| $body:block $( , $($rest:tt)* )? ) => {
        match &$region {
            SlotId::WellKnown(WellKnownSlot::Overlay) => { let $ctx = &$ctx_val; $body }
            _ => $crate::__slots_contribute_impl!($region, $ctx_val, $( $($rest)* )? )
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
            dirty::BUFFER
                | dirty::STATUS
                | dirty::MENU
                | dirty::INFO
                | dirty::OPTIONS
                | dirty::SESSION
                | dirty::SETTINGS
        );
    }

    #[test]
    fn dirty_flags_menu_is_composite() {
        assert_eq!(dirty::MENU, dirty::MENU_STRUCTURE | dirty::MENU_SELECTION);
    }

    #[test]
    fn dirty_all_matches_bitflags() {
        // SDK's ALL intentionally excludes PLUGIN_STATE (bit 7). Core ALL = 0x3FF, SDK ALL = 0x37F.
        assert_eq!(dirty::ALL, 0x37F);
    }

    #[test]
    fn test_plugin_state_constant() {
        assert_eq!(dirty::PLUGIN_STATE, 0x80);
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

    #[test]
    fn attribute_constants_match_core() {
        // Must match kasane_core::protocol::color::Attributes bitflags
        assert_eq!(attributes::UNDERLINE, 1 << 0);
        assert_eq!(attributes::CURLY_UNDERLINE, 1 << 1);
        assert_eq!(attributes::DOUBLE_UNDERLINE, 1 << 2);
        assert_eq!(attributes::REVERSE, 1 << 3);
        assert_eq!(attributes::BLINK, 1 << 4);
        assert_eq!(attributes::BOLD, 1 << 5);
        assert_eq!(attributes::DIM, 1 << 6);
        assert_eq!(attributes::ITALIC, 1 << 7);
        assert_eq!(attributes::STRIKETHROUGH, 1 << 8);
    }

    #[test]
    fn keys_push_literal_basic() {
        let mut k = Vec::new();
        keys::push_literal(&mut k, "abc");
        assert_eq!(k, vec!["a", "b", "c"]);
    }

    #[test]
    fn keys_push_literal_special_chars() {
        let mut k = Vec::new();
        keys::push_literal(&mut k, "a b<->%");
        assert_eq!(
            k,
            vec!["a", "<space>", "b", "<lt>", "<minus>", "<gt>", "<percent>"]
        );
    }

    #[test]
    fn keys_command_builds_esc_colon_cmd_ret() {
        let k = keys::command("edit foo.rs");
        assert_eq!(k[0], "<esc>");
        assert_eq!(k[1], ":");
        // "edit foo.rs" → "e","d","i","t","<space>","f","o","o",".","r","s"
        assert_eq!(k.last().unwrap(), "<ret>");
        assert!(k.contains(&"<space>".to_string()));
    }

    // --- interactive_id! tests ---

    // stride must be >= max packed value across all variants + 1.
    // Mixed: 3 byte fields → max = 255 + 255*256 + 1*65536 = 131071, so stride ≥ 131072 = 2^17
    interactive_id! {
        enum TestId(base = 1000, stride = 131072) {
            Simple,
            OneField { val: u8 },
            TwoFields { a: u8, b: u8 },
            WithBool { flag: bool },
            Mixed { idx: u8, ch: u8, down: bool },
        }
    }

    #[test]
    fn interactive_id_simple_roundtrip() {
        let id = TestId::Simple.encode();
        assert_eq!(id, 1000);
        assert_eq!(TestId::decode(id), Some(TestId::Simple));
    }

    #[test]
    fn interactive_id_one_field_roundtrip() {
        for v in [0u8, 1, 127, 255] {
            let id = TestId::OneField { val: v }.encode();
            assert_eq!(TestId::decode(id), Some(TestId::OneField { val: v }));
        }
    }

    #[test]
    fn interactive_id_two_fields_roundtrip() {
        let id = TestId::TwoFields { a: 42, b: 99 }.encode();
        assert_eq!(TestId::decode(id), Some(TestId::TwoFields { a: 42, b: 99 }));
    }

    #[test]
    fn interactive_id_bool_roundtrip() {
        for flag in [false, true] {
            let id = TestId::WithBool { flag }.encode();
            assert_eq!(TestId::decode(id), Some(TestId::WithBool { flag }));
        }
    }

    #[test]
    fn interactive_id_mixed_roundtrip() {
        for idx in 0..3u8 {
            for ch in 0..3u8 {
                for down in [false, true] {
                    let orig = TestId::Mixed { idx, ch, down };
                    let id = orig.encode();
                    assert_eq!(TestId::decode(id), Some(orig));
                }
            }
        }
    }

    #[test]
    fn interactive_id_below_base_returns_none() {
        assert_eq!(TestId::decode(999), None);
    }

    #[test]
    fn interactive_id_out_of_range_tag_returns_none() {
        // tag 5 does not exist (only 0..4)
        assert_eq!(TestId::decode(1000 + 5 * 131072), None);
    }

    // --- interactive_id! namespaced form (auto-stride) tests ---

    interactive_id! {
        enum AutoId {
            Simple,
            OneField { val: u8 },
            Mixed { idx: u8, ch: u8, down: bool },
        }
    }

    #[test]
    fn interactive_id_auto_stride_simple_roundtrip() {
        let id = AutoId::Simple.encode();
        assert_eq!(id, 0);
        assert_eq!(AutoId::decode(id), Some(AutoId::Simple));
    }

    #[test]
    fn interactive_id_auto_stride_field_roundtrip() {
        for v in [0u8, 1, 127, 255] {
            let id = AutoId::OneField { val: v }.encode();
            assert_eq!(AutoId::decode(id), Some(AutoId::OneField { val: v }));
        }
    }

    #[test]
    fn interactive_id_auto_stride_mixed_roundtrip() {
        for idx in 0..3u8 {
            for ch in 0..3u8 {
                for down in [false, true] {
                    let orig = AutoId::Mixed { idx, ch, down };
                    let id = orig.encode();
                    assert_eq!(AutoId::decode(id), Some(orig));
                }
            }
        }
    }

    #[test]
    fn interactive_id_auto_stride_bool_is_1_bit() {
        // Mixed has u8(8) + u8(8) + bool(1) = 17 bits → stride = 2^17 = 131072
        // AutoId::Mixed is variant tag 2, so base offset = 2 * 131072 = 262144
        let base = AutoId::Mixed {
            idx: 0,
            ch: 0,
            down: false,
        }
        .encode();
        let with_bool = AutoId::Mixed {
            idx: 0,
            ch: 0,
            down: true,
        }
        .encode();
        // bool is at bit position 16 (after u8+u8), so sets bit 16 = 65536
        assert_eq!(with_bool - base, 65536);
    }

    // --- JobTracker tests ---

    #[test]
    fn job_tracker_basic() {
        let tracker = job::JobTracker::new(100);
        assert_eq!(tracker.current_id(), 100);
        assert!(tracker.is_current(100));
        assert!(!tracker.is_current(101));
    }

    #[test]
    fn job_tracker_advance() {
        let mut tracker = job::JobTracker::new(100);
        let id2 = tracker.advance();
        assert_eq!(id2, 101);
        assert!(!tracker.is_current(100));
        assert!(tracker.is_current(101));
    }

    #[test]
    fn job_tracker_stale_rejected() {
        let mut tracker = job::JobTracker::new(100);
        let old_id = tracker.current_id();
        tracker.advance();
        assert!(!tracker.append_stdout(old_id, "stale data"));
        assert_eq!(tracker.take_output(), "");
    }

    #[test]
    fn job_tracker_current_accepted() {
        let mut tracker = job::JobTracker::new(100);
        let id = tracker.current_id();
        assert!(tracker.append_stdout(id, "hello "));
        assert!(tracker.append_stdout(id, "world"));
        assert_eq!(tracker.take_output(), "hello world");
    }

    #[test]
    fn job_tracker_advance_clears_buffer() {
        let mut tracker = job::JobTracker::new(100);
        let id = tracker.current_id();
        tracker.append_stdout(id, "old data");
        tracker.advance();
        assert_eq!(tracker.take_output(), "");
    }

    #[test]
    fn job_tracker_lines() {
        let mut tracker = job::JobTracker::new(100);
        let id = tracker.current_id();
        tracker.append_stdout(id, "a\nb\nc");
        let lines: Vec<&str> = tracker.lines().collect();
        assert_eq!(lines, vec!["a", "b", "c"]);
    }
}

/// Test harness for WASM plugins (feature-gated: `test-harness`).
///
/// Provides a mock host environment for unit-testing Kasane WASM plugins
/// without the full runtime. See [`test::TestHarness`] for usage.
#[cfg(feature = "test-harness")]
pub mod test;

#[cfg(all(test, feature = "test-harness"))]
mod test_harness_tests {
    use super::test::*;

    #[test]
    fn harness_default_state() {
        let h = TestHarness::new();
        let state = h.state();
        assert_eq!(state.cursor_line, 1);
        assert_eq!(state.cursor_col, 1);
        assert_eq!(state.cols, 80);
        assert_eq!(state.rows, 24);
        assert!(state.focused);
    }

    #[test]
    fn harness_set_cursor() {
        let mut h = TestHarness::new();
        h.set_cursor_line(42);
        h.set_cursor_col(10);
        assert_eq!(mock_host_state::get_cursor_line(), 42);
        assert_eq!(mock_host_state::get_cursor_col(), 10);
    }

    #[test]
    fn harness_set_selection_count() {
        let mut h = TestHarness::new();
        h.set_selection_count(5);
        assert_eq!(mock_host_state::get_selection_count(), 5);
    }

    #[test]
    fn harness_element_arena() {
        let h = TestHarness::new();
        let handle = mock_element_builder::create_text("hello", "default");
        let arena = h.arena();
        assert_eq!(arena.len(), 1);
        assert!(arena.get(handle).unwrap().contains("hello"));
    }

    #[test]
    fn harness_logs() {
        let mut h = TestHarness::new();
        mock_host_log::log_message(1, "test message");
        let logs = h.drain_logs();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, 1);
        assert_eq!(logs[0].message, "test message");
    }

    #[test]
    fn harness_cleanup_on_drop() {
        {
            let mut h = TestHarness::new();
            h.set_cursor_line(99);
            mock_element_builder::create_text("temp", "default");
        }
        // After drop, state should be reset
        assert_eq!(mock_host_state::get_cursor_line(), 1);
        let h = TestHarness::new();
        assert!(h.arena().is_empty());
    }
}
