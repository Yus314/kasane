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
//! `colored()`, `is_ctrl()`, `status_badge()`, `redraw()`, and `hex()`.
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
//!             Some(auto_contribution(text("★", default_face())))
//!         },
//!     }
//! }
//!
//! export!(MyPlugin);
//! ```
//!
//! `generate!()` emits WIT bindings and auto-imports common types (`Guest`,
//! `host_state`, `element_builder`, `types::*`) plus helper functions
//! (`default_face()`, `rgb()`, `face_bg()`, `plain()`, `colored()`, etc.).
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
    /// Session metadata changed (session added/removed/switched).
    pub const SESSION: u16 = 1 << 8;
    /// Composite: any buffer-related change (content or cursor).
    pub const BUFFER: u16 = BUFFER_CONTENT | BUFFER_CURSOR;
    /// Composite: any menu-related change (structure or selection).
    pub const MENU: u16 = MENU_STRUCTURE | MENU_SELECTION;
    /// All flags combined.
    pub const ALL: u16 = BUFFER | STATUS | MENU | INFO | OPTIONS | SESSION;
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

/// Attribute bitflags matching `kasane_core::protocol::color::Attributes`.
///
/// These are the user-facing text attributes (underline, bold, italic, etc.).
/// Use in the `attributes` field of a WIT `Face` struct.
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

/// `FaceMerge` mode constants for `CellDecoration`.
pub mod face_merge {
    /// Completely replace the existing cell face.
    pub const REPLACE: u8 = 0;
    /// Overlay non-default fields onto the existing face.
    pub const OVERLAY: u8 = 1;
    /// Only apply the background color from the decoration face.
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
/// - `kasane::plugin::types::*` — shared types (Face, Color, etc.)
///
/// Note: Guest crates must also depend on `wit-bindgen` directly, since
/// `wit_bindgen::generate!` generates code referencing `wit_bindgen` runtime types.
pub use kasane_plugin_sdk_macros::kasane_generate as generate;

/// Default lifecycle stubs.
///
/// Use inside a `Guest` trait impl to skip implementing unused lifecycle hooks.
/// For partial overrides, use `default_init!`, `default_shutdown!`, or
/// `default_state_changed!` individually.
#[macro_export]
macro_rules! default_lifecycle {
    () => {
        $crate::default_typed_lifecycle!();
    };
}

/// Default typed lifecycle stubs.
#[macro_export]
macro_rules! default_typed_lifecycle {
    () => {
        $crate::default_typed_init!();
        $crate::default_typed_active_session_ready!();
        $crate::default_typed_state_changed!();
        $crate::default_shutdown!();
    };
}

/// Default typed on_init stub.
#[macro_export]
macro_rules! default_typed_init {
    () => {
        fn on_init_effects() -> BootstrapEffects {
            BootstrapEffects::default()
        }
    };
}

/// Default typed active-session-ready stub.
#[macro_export]
macro_rules! default_typed_active_session_ready {
    () => {
        fn on_active_session_ready_effects() -> SessionReadyEffects {
            SessionReadyEffects::default()
        }
    };
}

/// Default typed on_state_changed stub.
#[macro_export]
macro_rules! default_typed_state_changed {
    () => {
        fn on_state_changed_effects(_dirty_flags: u16) -> RuntimeEffects {
            RuntimeEffects::default()
        }
    };
}

/// Default on_init stubs.
#[macro_export]
macro_rules! default_init {
    () => {
        $crate::default_typed_init!();
    };
}

/// Default active-session-ready stubs.
#[macro_export]
macro_rules! default_active_session_ready {
    () => {
        $crate::default_typed_active_session_ready!();
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

/// Default on_state_changed stubs.
#[macro_export]
macro_rules! default_state_changed {
    () => {
        $crate::default_typed_state_changed!();
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

/// Default caching stubs (state_hash returns 0).
#[macro_export]
macro_rules! default_cache {
    () => {
        fn state_hash() -> u64 {
            0
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

/// Default input handling stubs
/// (`handle_mouse`, `handle_key`, `handle_key_middleware`,
/// `handle_default_scroll`, `observe_key`, `observe_mouse`).
#[macro_export]
macro_rules! default_input {
    () => {
        fn handle_mouse(_event: MouseEvent, _id: InteractiveId) -> Option<Vec<Command>> {
            None
        }
        fn handle_key(_event: KeyEvent) -> Option<Vec<Command>> {
            None
        }
        fn handle_key_middleware(event: KeyEvent) -> KeyHandleResult {
            match Self::handle_key(event) {
                Some(commands) => KeyHandleResult::Consumed(commands),
                None => KeyHandleResult::Passthrough,
            }
        }
        fn handle_default_scroll(_candidate: DefaultScrollCandidate) -> Option<ScrollPolicyResult> {
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

/// Default cell decoration stub (returns empty list).
#[macro_export]
macro_rules! default_decorate_cells {
    () => {
        fn decorate_cells() -> Vec<CellDecoration> {
            Vec::new()
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
        $crate::default_typed_update!();
    };
}

/// Default typed runtime stubs.
#[macro_export]
macro_rules! default_typed_runtime {
    () => {
        $crate::default_typed_update!();
        $crate::default_typed_io_event!();
    };
}

/// Default typed update stub.
#[macro_export]
macro_rules! default_typed_update {
    () => {
        fn update_effects(_payload: Vec<u8>) -> RuntimeEffects {
            RuntimeEffects::default()
        }
    };
}

/// Default typed on-io-event stub.
#[macro_export]
macro_rules! default_typed_io_event {
    () => {
        fn on_io_event_effects(_event: IoEvent) -> RuntimeEffects {
            RuntimeEffects::default()
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

/// Default transform stub (passes through subject unchanged).
#[macro_export]
macro_rules! default_transform {
    () => {
        fn transform(
            _target: TransformTarget,
            subject: TransformSubject,
            _ctx: TransformContext,
        ) -> TransformSubject {
            subject
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

/// Default display-directives stub (returns no directives).
#[macro_export]
macro_rules! default_display_directives {
    () => {
        fn display_directives() -> Vec<DisplayDirective> {
            vec![]
        }
    };
}

/// Default workspace-changed stub (ignores workspace layout notifications).
#[macro_export]
macro_rules! default_workspace_changed {
    () => {
        fn on_workspace_changed(_snapshot: WorkspaceSnapshot) {}
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

/// Default requested-capabilities stub (returns empty list = no WASI capabilities).
#[macro_export]
macro_rules! default_capabilities {
    () => {
        fn requested_capabilities() -> Vec<Capability> {
            vec![]
        }
    };
}

/// Default requested-authorities stub (returns empty list = no privileged host authorities).
#[macro_export]
macro_rules! default_authorities {
    () => {
        fn requested_authorities() -> Vec<PluginAuthority> {
            vec![]
        }
    };
}

/// Default on-io-event stub (returns empty command list).
#[macro_export]
macro_rules! default_io_event {
    () => {
        $crate::default_typed_io_event!();
    };
}

/// Default view-deps stub (returns ALL = 0x17F).
#[macro_export]
macro_rules! default_view_deps {
    () => {
        fn view_deps() -> u16 {
            0x17F
        }
    };
}

/// Default key map stub (declare_key_map returns empty, is_group_active returns true, invoke_action returns Pass).
#[macro_export]
macro_rules! default_key_map {
    () => {
        fn declare_key_map() -> Vec<KeyGroupDecl> {
            Vec::new()
        }
        fn is_group_active(_group_name: String) -> bool {
            true
        }
        fn invoke_action(_action_id: String, _event: KeyEvent) -> KeyResponse {
            KeyResponse::Pass
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
            assert!(matches!(h.feed(1, IoEventKind::Stdout(b"hello")), ProcessResult::Pending));
            match h.feed(1, IoEventKind::Exited(0)) {
                ProcessResult::Completed(data) => assert_eq!(data, b"hello"),
                other => panic!("expected Completed, got {other:?}"),
            }
        }

        #[test]
        fn primary_exit_nonzero_empty_tries_fallback() {
            let mut h = ProcessHandle::new(1)
                .with_fallback(2, ProcessStep { program: "find".into(), args: vec![] });
            match h.feed(1, IoEventKind::Exited(1)) {
                ProcessResult::TryFallback => {}
                other => panic!("expected TryFallback, got {other:?}"),
            }
        }

        #[test]
        fn primary_spawn_failed_tries_fallback() {
            let mut h = ProcessHandle::new(1)
                .with_fallback(2, ProcessStep { program: "find".into(), args: vec![] });
            match h.feed(1, IoEventKind::SpawnFailed("not found")) {
                ProcessResult::TryFallback => {}
                other => panic!("expected TryFallback, got {other:?}"),
            }
        }

        #[test]
        fn fallback_completes() {
            let mut h = ProcessHandle::new(1)
                .with_fallback(2, ProcessStep { program: "find".into(), args: vec![] });
            // Primary fails
            assert!(matches!(h.feed(1, IoEventKind::SpawnFailed("nope")), ProcessResult::TryFallback));
            // Fallback produces output
            assert!(matches!(h.feed(2, IoEventKind::Stdout(b"file.txt\n")), ProcessResult::Pending));
            match h.feed(2, IoEventKind::Exited(0)) {
                ProcessResult::Completed(data) => assert_eq!(data, b"file.txt\n"),
                other => panic!("expected Completed, got {other:?}"),
            }
        }

        #[test]
        fn fallback_fails() {
            let mut h = ProcessHandle::new(1)
                .with_fallback(2, ProcessStep { program: "find".into(), args: vec![] });
            assert!(matches!(h.feed(1, IoEventKind::SpawnFailed("nope")), ProcessResult::TryFallback));
            match h.feed(2, IoEventKind::SpawnFailed("also not found")) {
                ProcessResult::Failed(msg) => assert!(msg.contains("also not found")),
                other => panic!("expected Failed, got {other:?}"),
            }
        }

        #[test]
        fn unknown_job_id_ignored() {
            let mut h = ProcessHandle::new(1);
            assert!(matches!(h.feed(99, IoEventKind::Stdout(b"x")), ProcessResult::Ignored));
        }

        #[test]
        fn stderr_is_pending() {
            let mut h = ProcessHandle::new(1);
            assert!(matches!(h.feed(1, IoEventKind::Stderr(b"warn")), ProcessResult::Pending));
        }

        #[test]
        fn primary_exit_nonzero_with_data_completes() {
            let mut h = ProcessHandle::new(1);
            assert!(matches!(h.feed(1, IoEventKind::Stdout(b"partial")), ProcessResult::Pending));
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
            let mut h = ProcessHandle::new(1)
                .with_fallback(2, ProcessStep { program: "find".into(), args: vec![] });
            h.feed(1, IoEventKind::SpawnFailed("nope"));
            h.reset();
            // After reset, primary events should work again
            assert!(matches!(h.feed(1, IoEventKind::Stdout(b"ok")), ProcessResult::Pending));
        }

        #[test]
        fn fallback_info_present() {
            let step = ProcessStep { program: "find".into(), args: vec![".".into()] };
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
///             Some(auto_contribution(text("hello", default_face())))
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
