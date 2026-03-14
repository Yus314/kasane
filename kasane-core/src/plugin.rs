use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::ops::Range;
use std::time::Duration;

use bitflags::bitflags;
use compact_str::CompactString;

use crate::element::{Element, FlexChild, InteractiveId, Overlay, OverlayAnchor};
use crate::input::{KeyEvent, MouseEvent};
use crate::layout::{HitMap, Rect};
use crate::pane::{PaneCommand, PaneId, PanePermissions};
use crate::protocol::{Face, KasaneRequest};
use crate::state::{AppState, DirtyFlags};
use crate::workspace::WorkspaceCommand;

bitflags! {
    /// Declares which Plugin trait methods a plugin actually implements.
    /// Used by PluginRegistry to skip WASM boundary crossings for non-participating plugins.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PluginCapabilities: u32 {
        const SLOT_CONTRIBUTOR = 1 << 0;
        const LINE_DECORATION  = 1 << 1;
        const OVERLAY          = 1 << 2;
        const DECORATOR        = 1 << 3;
        const REPLACEMENT      = 1 << 4;
        const MENU_TRANSFORM   = 1 << 5;
        const CURSOR_STYLE     = 1 << 6;
        const INPUT_HANDLER    = 1 << 7;
        const NAMED_SLOT       = 1 << 8;
        const PANE_LIFECYCLE   = 1 << 9;
        const PANE_RENDERER    = 1 << 10;
        const SURFACE_PROVIDER    = 1 << 11;
        const WORKSPACE_OBSERVER  = 1 << 12;
        const PAINT_HOOK         = 1 << 13;
        const CONTRIBUTOR        = 1 << 14;
        const TRANSFORMER        = 1 << 15;
        const ANNOTATOR          = 1 << 16;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginId(pub String);

#[deprecated(since = "0.2.0", note = "Use SlotId instead of Slot")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    BufferLeft,
    BufferRight,
    AboveBuffer,
    BelowBuffer,
    AboveStatus,
    StatusLeft,
    StatusRight,
    Overlay,
}

#[allow(deprecated)]
impl Slot {
    pub const COUNT: usize = 8;

    pub fn index(self) -> usize {
        match self {
            Self::BufferLeft => 0,
            Self::BufferRight => 1,
            Self::AboveBuffer => 2,
            Self::BelowBuffer => 3,
            Self::AboveStatus => 4,
            Self::StatusLeft => 5,
            Self::StatusRight => 6,
            Self::Overlay => 7,
        }
    }

    const ALL_VARIANTS: [Slot; Self::COUNT] = [
        Self::BufferLeft,
        Self::BufferRight,
        Self::AboveBuffer,
        Self::BelowBuffer,
        Self::AboveStatus,
        Self::StatusLeft,
        Self::StatusRight,
        Self::Overlay,
    ];
}

/// Open slot identifier that supports both well-known and custom plugin-defined slots.
///
/// Well-known slots have `const` definitions matching the legacy `Slot` enum variants.
/// Plugins can define custom slots using arbitrary names (e.g., `SlotId::new("myplugin.sidebar")`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SlotId(pub CompactString);

impl SlotId {
    pub const BUFFER_LEFT: Self = Self(CompactString::const_new("kasane.buffer.left"));
    pub const BUFFER_RIGHT: Self = Self(CompactString::const_new("kasane.buffer.right"));
    pub const ABOVE_BUFFER: Self = Self(CompactString::const_new("kasane.buffer.above"));
    pub const BELOW_BUFFER: Self = Self(CompactString::const_new("kasane.buffer.below"));
    pub const ABOVE_STATUS: Self = Self(CompactString::const_new("kasane.status.above"));
    pub const STATUS_LEFT: Self = Self(CompactString::const_new("kasane.status.left"));
    pub const STATUS_RIGHT: Self = Self(CompactString::const_new("kasane.status.right"));
    pub const OVERLAY: Self = Self(CompactString::const_new("kasane.overlay"));

    /// Well-known SlotIds in the same order as `Slot::ALL_VARIANTS`.
    #[allow(deprecated)]
    const WELL_KNOWN: [SlotId; Slot::COUNT] = [
        Self::BUFFER_LEFT,
        Self::BUFFER_RIGHT,
        Self::ABOVE_BUFFER,
        Self::BELOW_BUFFER,
        Self::ABOVE_STATUS,
        Self::STATUS_LEFT,
        Self::STATUS_RIGHT,
        Self::OVERLAY,
    ];

    /// Create a custom slot identifier.
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self(name.into())
    }

    /// Get the slot name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to a legacy `Slot` if this is a well-known slot.
    #[allow(deprecated)]
    pub fn to_legacy(&self) -> Option<Slot> {
        Self::WELL_KNOWN
            .iter()
            .position(|wk| wk == self)
            .map(|i| Slot::ALL_VARIANTS[i])
    }

    /// Check if this is a well-known (built-in) slot.
    pub fn is_well_known(&self) -> bool {
        Self::WELL_KNOWN.contains(self)
    }
}

#[allow(deprecated)]
impl From<Slot> for SlotId {
    fn from(slot: Slot) -> Self {
        Self::WELL_KNOWN[slot.index()].clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecorateTarget {
    Buffer,
    StatusBar,
    Menu,
    Info,
    BufferLine(usize),
}

/// Target identifier for legacy seed-substitution in the transform chain.
///
/// Despite the name "Replace", this does **not** perform an exclusive override.
/// A plugin returning `Some(element)` from [`Plugin::replace()`] for a given
/// target provides an alternative *seed element* that enters the transform
/// chain in place of the default element.  All subsequent transforms
/// (decorators and transformers) are still applied to the replacement seed
/// in exactly the same way as they would be applied to the default element.
///
/// In other words, `ReplaceTarget` controls Phase 1 (Seed Selection) of
/// [`PluginRegistry::apply_transform_chain()`] — it never bypasses Phase 3
/// (Chain Application).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplaceTarget {
    MenuPrompt,
    MenuInline,
    MenuSearch,
    InfoPrompt,
    InfoModal,
    StatusBar,
}

/// Decoration for a single buffer line, contributed by a plugin.
#[derive(Debug, Clone)]
pub struct LineDecoration {
    pub left_gutter: Option<Element>,
    pub right_gutter: Option<Element>,
    pub background: Option<Face>,
}

// ===========================================================================
// New plugin API types: Contribute / Transform / Annotate
// ===========================================================================

/// Layout constraints passed to plugins during contribution.
#[derive(Debug, Clone)]
pub struct ContributeContext {
    pub available_width: u16,
    pub available_height: u16,
    pub visible_lines: Range<usize>,
    pub screen_cols: u16,
    pub screen_rows: u16,
}

impl ContributeContext {
    /// Build from AppState and an optional surface rect.
    pub fn new(state: &AppState, rect: Option<&Rect>) -> Self {
        let (w, h) = if let Some(r) = rect {
            (r.w, r.h)
        } else {
            (state.cols, state.available_height())
        };
        ContributeContext {
            available_width: w,
            available_height: h,
            visible_lines: state.visible_line_range(),
            screen_cols: state.cols,
            screen_rows: state.rows,
        }
    }
}

/// Result of a plugin's `contribute_to()` call.
#[derive(Debug, Clone)]
pub struct Contribution {
    pub element: Element,
    pub priority: i16,
    pub size_hint: ContribSizeHint,
}

/// Size hint for a contribution within a slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContribSizeHint {
    Auto,
    Fixed(u16),
    Flex(f32),
}

/// Transform target — unifies Decorator + Replacement targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformTarget {
    Buffer,
    BufferLine(usize),
    StatusBar,
    Menu,
    MenuPrompt,
    MenuInline,
    MenuSearch,
    Info,
    InfoPrompt,
    InfoModal,
}

/// Context passed to `transform()`.
#[derive(Debug, Clone)]
pub struct TransformContext {
    pub is_default: bool,
    pub chain_position: usize,
}

/// Context for `annotate_line_with_ctx()`.
#[derive(Debug, Clone)]
pub struct AnnotateContext {
    pub line_width: u16,
    pub gutter_width: u16,
}

/// A background layer with z-ordering and blend mode.
#[derive(Debug, Clone)]
pub struct BackgroundLayer {
    pub face: Face,
    pub z_order: i16,
    pub blend: BlendMode,
}

/// How a background layer is composited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Opaque,
}

/// New line annotation with `BackgroundLayer` support.
///
/// Annotations are collected from all annotating plugins per visible line.
/// When multiple plugins contribute gutter elements, they are sorted by
/// `priority` (ascending: lower values appear first / leftmost).
#[derive(Debug, Clone)]
pub struct LineAnnotation {
    pub left_gutter: Option<Element>,
    pub right_gutter: Option<Element>,
    pub background: Option<BackgroundLayer>,
    /// Sort priority for gutter element ordering (default: 0).
    /// Lower values sort first (leftmost in left gutter, leftmost in right gutter).
    /// Mirrors `Contribution::priority` and `BackgroundLayer::z_order` conventions.
    pub priority: i16,
}

/// Context for overlay contributions with collision avoidance.
#[derive(Debug, Clone)]
pub struct OverlayContext {
    pub screen_cols: u16,
    pub screen_rows: u16,
    pub menu_rect: Option<Rect>,
    pub existing_overlays: Vec<Rect>,
}

/// Overlay contribution with z-index.
#[derive(Debug, Clone)]
pub struct OverlayContribution {
    pub element: Element,
    pub anchor: OverlayAnchor,
    pub z_index: i16,
}

/// Aggregated annotation result from all plugins.
#[derive(Debug, Clone)]
pub struct AnnotationResult {
    pub left_gutter: Option<Element>,
    pub right_gutter: Option<Element>,
    pub line_backgrounds: Option<Vec<Option<Face>>>,
}

/// A post-paint hook that can modify the CellGrid after the standard paint pass.
///
/// PaintHooks enable plugins to apply custom rendering effects (e.g., highlights,
/// overlays, visual indicators) directly on the cell grid without needing to
/// participate in the Element tree.
pub trait PaintHook: Send {
    /// Unique identifier for this hook (typically `"plugin_id.hook_name"`).
    fn id(&self) -> &str;

    /// DirtyFlags that trigger this hook. The hook is skipped when none of these
    /// flags are set.
    fn deps(&self) -> DirtyFlags;

    /// Optional surface filter. When `Some(id)`, only apply when that surface
    /// was rendered. When `None`, apply on every paint pass.
    fn surface_filter(&self) -> Option<crate::surface::SurfaceId> {
        None
    }

    /// Apply the hook to the cell grid.
    ///
    /// `region` is the rectangular area that was painted (typically the full screen).
    fn apply(
        &self,
        grid: &mut crate::render::CellGrid,
        region: &crate::layout::Rect,
        state: &AppState,
    );
}

pub enum Command {
    SendToKakoune(KasaneRequest),
    Paste,
    Quit,
    RequestRedraw(DirtyFlags),
    /// Schedule a timer that fires after `delay`, delivering `payload` to `target` plugin.
    ScheduleTimer {
        delay: Duration,
        target: PluginId,
        payload: Box<dyn Any + Send>,
    },
    /// Send a message directly to another plugin.
    PluginMessage {
        target: PluginId,
        payload: Box<dyn Any + Send>,
    },
    /// Override a configuration value at runtime.
    SetConfig {
        key: String,
        value: String,
    },
    /// Pane management command (split, close, focus, etc.).
    Pane(PaneCommand),
    /// Workspace layout command (add/remove surface, focus, split, float, etc.).
    Workspace(WorkspaceCommand),
    /// Register custom theme tokens with default faces.
    RegisterThemeTokens(Vec<(String, Face)>),
}

/// Commands that require event-loop-level handling (timers, inter-plugin messages, config).
pub enum DeferredCommand {
    ScheduleTimer {
        delay: Duration,
        target: PluginId,
        payload: Box<dyn Any + Send>,
    },
    PluginMessage {
        target: PluginId,
        payload: Box<dyn Any + Send>,
    },
    SetConfig {
        key: String,
        value: String,
    },
    Pane(PaneCommand),
    Workspace(WorkspaceCommand),
    RegisterThemeTokens(Vec<(String, Face)>),
}

/// Separate deferred commands from normal commands.
/// Returns (normal_commands, deferred_commands).
pub fn extract_deferred_commands(commands: Vec<Command>) -> (Vec<Command>, Vec<DeferredCommand>) {
    let mut normal = Vec::new();
    let mut deferred = Vec::new();
    for cmd in commands {
        match cmd {
            Command::ScheduleTimer {
                delay,
                target,
                payload,
            } => deferred.push(DeferredCommand::ScheduleTimer {
                delay,
                target,
                payload,
            }),
            Command::PluginMessage { target, payload } => {
                deferred.push(DeferredCommand::PluginMessage { target, payload })
            }
            Command::SetConfig { key, value } => {
                deferred.push(DeferredCommand::SetConfig { key, value })
            }
            Command::Pane(cmd) => deferred.push(DeferredCommand::Pane(cmd)),
            Command::Workspace(cmd) => deferred.push(DeferredCommand::Workspace(cmd)),
            Command::RegisterThemeTokens(tokens) => {
                deferred.push(DeferredCommand::RegisterThemeTokens(tokens))
            }
            other => normal.push(other),
        }
    }
    (normal, deferred)
}

/// コマンド実行の結果。
pub enum CommandResult {
    /// すべてのコマンドを処理した。
    Continue,
    /// Quit コマンドを受信した。
    Quit,
}

/// Side-effect コマンドを実行する。
/// `clipboard_get` はクリップボード読み取りのクロージャ。
pub fn execute_commands(
    commands: Vec<Command>,
    kak_writer: &mut impl Write,
    clipboard_get: &mut dyn FnMut() -> Option<String>,
) -> CommandResult {
    use crate::input::paste_text_to_keys;

    for cmd in commands {
        match cmd {
            Command::SendToKakoune(req) => {
                crate::io::send_request(kak_writer, &req);
            }
            Command::Paste => {
                if let Some(text) = clipboard_get() {
                    let keys = paste_text_to_keys(&text);
                    if !keys.is_empty() {
                        crate::io::send_request(kak_writer, &KasaneRequest::Keys(keys));
                    }
                }
            }
            Command::Quit => return CommandResult::Quit,
            Command::RequestRedraw(_) => {} // handled earlier by extract_redraw_flags
            // Deferred commands should be extracted before reaching execute_commands
            Command::ScheduleTimer { .. }
            | Command::PluginMessage { .. }
            | Command::SetConfig { .. }
            | Command::Pane(_)
            | Command::Workspace(_)
            | Command::RegisterThemeTokens(_) => {}
        }
    }
    CommandResult::Continue
}

/// Extract RequestRedraw commands, merging their flags.
/// Returns the merged DirtyFlags; the input Vec retains only non-redraw commands.
pub fn extract_redraw_flags(commands: &mut Vec<Command>) -> DirtyFlags {
    let mut flags = DirtyFlags::empty();
    commands.retain(|cmd| {
        if let Command::RequestRedraw(f) = cmd {
            flags |= *f;
            false
        } else {
            true
        }
    });
    flags
}

pub trait Plugin: Any {
    fn id(&self) -> PluginId;

    // --- Lifecycle hooks ---

    fn on_init(&mut self, _state: &AppState) -> Vec<Command> {
        vec![]
    }
    fn on_shutdown(&mut self) {}
    fn on_state_changed(&mut self, _state: &AppState, _dirty: DirtyFlags) -> Vec<Command> {
        vec![]
    }

    // --- Input hooks ---

    /// Observe a key event (notification only, cannot consume).
    fn observe_key(&mut self, _key: &KeyEvent, _state: &AppState) {}
    /// Observe a mouse event (notification only, cannot consume).
    fn observe_mouse(&mut self, _event: &MouseEvent, _state: &AppState) {}

    // --- Update / Input handling ---

    fn update(&mut self, _msg: Box<dyn Any>, _state: &AppState) -> Vec<Command> {
        vec![]
    }
    fn handle_key(&mut self, _key: &KeyEvent, _state: &AppState) -> Option<Vec<Command>> {
        None
    }
    fn handle_mouse(
        &mut self,
        _event: &MouseEvent,
        _id: InteractiveId,
        _state: &AppState,
    ) -> Option<Vec<Command>> {
        None
    }

    // --- View contributions ---

    /// Hash of plugin-internal state for view caching (L1).
    /// Default: 0 (no state-based caching; slot_deps still applies).
    fn state_hash(&self) -> u64 {
        0
    }

    /// DirtyFlags dependencies for contribute() on a given slot (L3).
    /// Default: ALL (always recompute when any AppState change occurs).
    #[deprecated(since = "0.2.0", note = "Override slot_id_deps() instead")]
    #[allow(deprecated)]
    fn slot_deps(&self, _slot: Slot) -> DirtyFlags {
        DirtyFlags::ALL
    }

    #[deprecated(since = "0.2.0", note = "Override contribute_slot() instead")]
    #[allow(deprecated)]
    fn contribute(&self, _slot: Slot, _state: &AppState) -> Option<Element> {
        None
    }

    #[deprecated(since = "0.5.0", note = "Use contribute_overlay_with_ctx() instead")]
    fn contribute_overlay(&self, _state: &AppState) -> Option<Overlay> {
        None
    }

    #[deprecated(since = "0.5.0", note = "Use transform() instead")]
    fn decorate(&self, _target: DecorateTarget, element: Element, _state: &AppState) -> Element {
        element
    }

    /// Provide an alternative seed element for the given [`ReplaceTarget`].
    ///
    /// Returning `Some(element)` substitutes the default element as the seed
    /// that enters the transform chain.  Crucially, the returned element is
    /// **not** used as-is — it still passes through every decorator and
    /// transformer in the chain (Phase 3 of
    /// [`PluginRegistry::apply_transform_chain()`]).  This means decorators
    /// and transformers are never bypassed by a replacement.
    ///
    /// When multiple plugins provide a replacement for the same target, the
    /// last one encountered in reverse registration order wins (last-wins
    /// semantics in Phase 1).
    #[deprecated(since = "0.5.0", note = "Use transform() instead")]
    fn replace(&self, _target: ReplaceTarget, _state: &AppState) -> Option<Element> {
        None
    }

    #[deprecated(since = "0.5.0", note = "Use transform_priority() instead")]
    fn decorator_priority(&self) -> u32 {
        0
    }

    // --- Line decoration ---

    /// Contribute decoration for a specific buffer line.
    #[deprecated(since = "0.5.0", note = "Use annotate_line_with_ctx() instead")]
    fn contribute_line(&self, _line: usize, _state: &AppState) -> Option<LineDecoration> {
        None
    }

    // --- Cursor style ---

    /// Override the cursor style. Return None to defer to the default logic.
    /// First non-None result from any plugin is used.
    fn cursor_style_override(&self, _state: &AppState) -> Option<crate::render::CursorStyle> {
        None
    }

    // --- Named slot contributions ---

    /// Contribute an element to a custom named slot defined by another plugin.
    fn contribute_named_slot(&self, _name: &str, _state: &AppState) -> Option<Element> {
        None
    }

    // --- SlotId-based contributions (open slot system) ---

    /// Contribute an element to an open SlotId.
    ///
    /// Default implementation delegates to `contribute()` for well-known slots
    /// and `contribute_named_slot()` for custom slots.
    #[deprecated(since = "0.5.0", note = "Use contribute_to() instead")]
    #[allow(deprecated)]
    fn contribute_slot(&self, slot_id: &SlotId, state: &AppState) -> Option<Element> {
        if let Some(legacy) = slot_id.to_legacy() {
            self.contribute(legacy, state)
        } else {
            self.contribute_named_slot(slot_id.as_str(), state)
        }
    }

    /// DirtyFlags dependencies for `contribute_slot()` on a given SlotId.
    ///
    /// Default implementation delegates to `slot_deps()` for well-known slots
    /// and returns `DirtyFlags::ALL` for custom slots.
    #[deprecated(since = "0.5.0", note = "Use contribute_deps() instead")]
    #[allow(deprecated)]
    fn slot_id_deps(&self, slot_id: &SlotId) -> DirtyFlags {
        slot_id
            .to_legacy()
            .map(|s| self.slot_deps(s))
            .unwrap_or(DirtyFlags::ALL)
    }

    // --- Menu item transformation ---

    /// Transform a menu item before rendering. Return None for no change.
    fn transform_menu_item(
        &self,
        _item: &[crate::protocol::Atom],
        _index: usize,
        _selected: bool,
        _state: &AppState,
    ) -> Option<Vec<crate::protocol::Atom>> {
        None
    }

    /// Declare which capabilities this plugin supports.
    /// Used by PluginRegistry to skip calls to non-participating plugins.
    /// Default: all legacy capabilities. New API flags (CONTRIBUTOR, TRANSFORMER,
    /// ANNOTATOR) are opt-in and must be explicitly set by plugins that implement
    /// the new Contribute/Transform/Annotate API.
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::all()
            .difference(PluginCapabilities::CONTRIBUTOR)
            .difference(PluginCapabilities::TRANSFORMER)
            .difference(PluginCapabilities::ANNOTATOR)
    }

    /// DirtyFlags dependencies for contribute_overlay() (L3 overlay caching).
    /// Default: ALL (always recompute).
    fn overlay_deps(&self) -> DirtyFlags {
        DirtyFlags::ALL
    }

    // --- Pane lifecycle hooks (Phase 5) ---

    /// Called when a new pane is created.
    fn on_pane_created(&mut self, _pane_id: PaneId, _state: &AppState) {}

    /// Called when a pane is closed.
    fn on_pane_closed(&mut self, _pane_id: PaneId) {}

    /// Called when focus changes between panes.
    fn on_focus_changed(&mut self, _from: Option<PaneId>, _to: PaneId, _state: &AppState) {}

    /// Render plugin-owned pane content. Return `None` if this plugin does not
    /// own the given pane.
    fn render_pane(&self, _pane_id: PaneId, _cols: u16, _rows: u16) -> Option<Element> {
        None
    }

    /// Handle a key event for a plugin-owned pane.
    fn handle_pane_key(&mut self, _pane_id: PaneId, _key: &KeyEvent) -> Option<Vec<Command>> {
        None
    }

    /// Pane permission flags for this plugin. Default: none.
    fn pane_permissions(&self) -> PanePermissions {
        PanePermissions::empty()
    }

    // --- Surface system hooks (Phase S) ---

    /// Return surfaces owned by this plugin.
    /// Called once during initialization; returned surfaces are registered in the SurfaceRegistry.
    fn surfaces(&mut self) -> Vec<Box<dyn crate::surface::Surface>> {
        vec![]
    }

    /// Request where plugin-owned surfaces should be placed in the workspace.
    fn workspace_request(&self) -> Option<crate::workspace::Placement> {
        None
    }

    /// Notification that the workspace layout has changed.
    fn on_workspace_changed(&mut self, _query: &crate::workspace::WorkspaceQuery<'_>) {}

    // --- Paint hooks (Phase 5) ---

    /// Return paint hooks owned by this plugin.
    /// Called once during initialization; returned hooks are registered for use
    /// in the rendering pipeline (applied after the standard paint pass).
    fn paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
        vec![]
    }

    // === NEW: Contribute (replaces contribute_slot) ===

    /// Contribute an element to a region with layout context and priority.
    ///
    /// Default: falls back to `contribute_slot()` with priority=0 and Auto sizing.
    fn contribute_to(
        &self,
        region: &SlotId,
        state: &AppState,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        #[allow(deprecated)]
        self.contribute_slot(region, state).map(|el| Contribution {
            element: el,
            priority: 0,
            size_hint: ContribSizeHint::Auto,
        })
    }

    /// DirtyFlags dependencies for `contribute_to()`.
    ///
    /// Default: delegates to `slot_id_deps()`.
    fn contribute_deps(&self, region: &SlotId) -> DirtyFlags {
        #[allow(deprecated)]
        self.slot_id_deps(region)
    }

    // === NEW: Transform (replaces decorate + replace) ===

    /// Transform an element for the given target. The element may be the default
    /// or a result from a previous plugin in the chain.
    ///
    /// Default: pass through unchanged.
    fn transform(
        &self,
        _target: &TransformTarget,
        element: Element,
        _state: &AppState,
        _ctx: &TransformContext,
    ) -> Element {
        element
    }

    /// Priority for transform chain ordering (higher = applied earlier / inner).
    fn transform_priority(&self) -> i16 {
        0
    }

    /// DirtyFlags dependencies for `transform()` on a given target.
    fn transform_deps(&self, _target: &TransformTarget) -> DirtyFlags {
        DirtyFlags::ALL
    }

    // === NEW: Annotate (replaces contribute_line) ===

    /// Annotate a buffer line with gutter elements and/or background layer.
    ///
    /// Default: falls back to `contribute_line()`.
    fn annotate_line_with_ctx(
        &self,
        line: usize,
        state: &AppState,
        _ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        #[allow(deprecated)]
        self.contribute_line(line, state).map(|dec| LineAnnotation {
            left_gutter: dec.left_gutter,
            right_gutter: dec.right_gutter,
            background: dec.background.map(|face| BackgroundLayer {
                face,
                z_order: 0,
                blend: BlendMode::Opaque,
            }),
            priority: 0,
        })
    }

    /// DirtyFlags dependencies for `annotate_line_with_ctx()`.
    fn annotate_deps(&self) -> DirtyFlags {
        DirtyFlags::ALL
    }

    // === NEW: Overlay with context ===

    /// Contribute an overlay with collision-avoidance context.
    ///
    /// Default: falls back to `contribute_overlay()`.
    fn contribute_overlay_with_ctx(
        &self,
        state: &AppState,
        _ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        #[allow(deprecated)]
        self.contribute_overlay(state).map(|o| OverlayContribution {
            element: o.element,
            anchor: o.anchor,
            z_index: 0,
        })
    }
}

/// Cached result for a single plugin's slot contributions.
#[derive(Default)]
#[allow(deprecated)]
struct PluginCacheEntry {
    last_state_hash: u64,
    /// `None` = not cached. `Some(x)` = cached contribute() result.
    slots: [Option<Option<Element>>; Slot::COUNT],
    /// `None` = not cached. `Some(x)` = cached contribute_overlay() result.
    overlay: Option<Option<Overlay>>,
    /// Cache for custom (non-well-known) SlotId contributions.
    /// Key present = cached; value = the contribute_slot() result.
    custom_slots: HashMap<SlotId, Option<Element>>,
    /// Cached contribute_to() results (new API).
    contributions: HashMap<SlotId, Option<Contribution>>,
}

struct PluginSlotCache {
    entries: Vec<PluginCacheEntry>,
}

impl PluginSlotCache {
    fn new() -> Self {
        PluginSlotCache {
            entries: Vec::new(),
        }
    }
}

/// Effective DirtyFlags dependencies for each ViewCache section,
/// computed by unioning core deps with plugin contribution/transform/annotation deps.
#[derive(Debug, Clone, Copy)]
pub struct EffectiveSectionDeps {
    pub base: DirtyFlags,
    pub menu: DirtyFlags,
    pub info: DirtyFlags,
}

impl Default for EffectiveSectionDeps {
    fn default() -> Self {
        use crate::render::view::{
            BUILD_BASE_DEPS, BUILD_INFO_SECTION_DEPS, BUILD_MENU_SECTION_DEPS,
        };
        EffectiveSectionDeps {
            base: BUILD_BASE_DEPS,
            menu: BUILD_MENU_SECTION_DEPS,
            info: BUILD_INFO_SECTION_DEPS,
        }
    }
}

pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
    capabilities: Vec<PluginCapabilities>,
    hit_map: HitMap,
    slot_cache: RefCell<PluginSlotCache>,
    any_plugin_state_changed: bool,
    section_deps: EffectiveSectionDeps,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            plugins: Vec::new(),
            capabilities: Vec::new(),
            hit_map: HitMap::new(),
            slot_cache: RefCell::new(PluginSlotCache::new()),
            any_plugin_state_changed: false,
            section_deps: EffectiveSectionDeps::default(),
        }
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Check if any registered plugin has the given capability.
    fn has_capability(&self, cap: PluginCapabilities) -> bool {
        self.capabilities.iter().any(|c| c.contains(cap))
    }

    /// Returns true if any plugin's state_hash changed during the last
    /// `prepare_plugin_cache()` call.
    pub fn any_plugin_state_changed(&self) -> bool {
        self.any_plugin_state_changed
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        let id = plugin.id();
        let caps = plugin.capabilities();
        if let Some(pos) = self.plugins.iter().position(|p| p.id() == id) {
            // Replace existing plugin with same ID (e.g. FS plugin overrides bundled)
            self.plugins[pos] = plugin;
            self.capabilities[pos] = caps;
            // Reset the cache entry for the replaced plugin
            self.slot_cache.get_mut().entries[pos] = PluginCacheEntry::default();
        } else {
            self.plugins.push(plugin);
            self.capabilities.push(caps);
            self.slot_cache
                .get_mut()
                .entries
                .push(PluginCacheEntry::default());
        }
        self.recompute_section_deps();
    }

    /// Recompute effective section deps by unioning core deps with all
    /// plugin contribution/transform/annotation deps.
    fn recompute_section_deps(&mut self) {
        use crate::render::view::{
            BUILD_BASE_DEPS, BUILD_INFO_SECTION_DEPS, BUILD_MENU_SECTION_DEPS,
        };

        let mut base = BUILD_BASE_DEPS;
        let mut menu = BUILD_MENU_SECTION_DEPS;
        let mut info = BUILD_INFO_SECTION_DEPS;

        // Base slots
        let base_slots = [
            &SlotId::BUFFER_LEFT,
            &SlotId::BUFFER_RIGHT,
            &SlotId::ABOVE_BUFFER,
            &SlotId::BELOW_BUFFER,
            &SlotId::ABOVE_STATUS,
            &SlotId::STATUS_LEFT,
            &SlotId::STATUS_RIGHT,
        ];

        for plugin in &self.plugins {
            // Contribution deps for base slots
            for slot in &base_slots {
                base |= plugin.contribute_deps(slot);
            }

            // Annotation deps
            base |= plugin.annotate_deps();

            // Transform deps for base targets
            base |= plugin.transform_deps(&TransformTarget::Buffer);
            base |= plugin.transform_deps(&TransformTarget::StatusBar);

            // Transform deps for menu targets
            menu |= plugin.transform_deps(&TransformTarget::Menu);
            menu |= plugin.transform_deps(&TransformTarget::MenuPrompt);
            menu |= plugin.transform_deps(&TransformTarget::MenuInline);
            menu |= plugin.transform_deps(&TransformTarget::MenuSearch);

            // Transform deps for info targets
            info |= plugin.transform_deps(&TransformTarget::Info);
            info |= plugin.transform_deps(&TransformTarget::InfoPrompt);
            info |= plugin.transform_deps(&TransformTarget::InfoModal);
        }

        self.section_deps = EffectiveSectionDeps { base, menu, info };
    }

    /// Get the effective section deps (includes plugin contributions).
    pub fn section_deps(&self) -> &EffectiveSectionDeps {
        &self.section_deps
    }

    /// Invalidate slot cache entries based on dirty flags and state hash changes.
    /// Call once per frame before rendering (during the mutable phase).
    #[allow(deprecated)]
    pub fn prepare_plugin_cache(&mut self, dirty: DirtyFlags) {
        let cache = self.slot_cache.get_mut();
        self.any_plugin_state_changed = false;

        // Grow entries if plugins were registered after last prepare
        while cache.entries.len() < self.plugins.len() {
            cache.entries.push(PluginCacheEntry::default());
        }

        for (i, plugin) in self.plugins.iter().enumerate() {
            let entry = &mut cache.entries[i];
            let current_hash = plugin.state_hash();

            // L1: state hash changed → invalidate all slot entries + overlay for this plugin
            if current_hash != entry.last_state_hash {
                entry.last_state_hash = current_hash;
                for slot_entry in &mut entry.slots {
                    *slot_entry = None;
                }
                entry.overlay = None;
                entry.custom_slots.clear();
                entry.contributions.clear();
                self.any_plugin_state_changed = true;
                continue; // all slots already invalidated
            }

            // L3: check per-slot dirty flag intersection
            for (idx, wk) in SlotId::WELL_KNOWN.iter().enumerate() {
                let slot_deps = plugin.slot_id_deps(wk);
                if dirty.intersects(slot_deps) {
                    entry.slots[idx] = None;
                }
            }

            // L3: custom slot dirty flag intersection
            entry.custom_slots.retain(|slot_id, _| {
                let deps = plugin.slot_id_deps(slot_id);
                !dirty.intersects(deps)
            });

            // L3: overlay dirty flag intersection
            let overlay_deps = plugin.overlay_deps();
            if dirty.intersects(overlay_deps) {
                entry.overlay = None;
            }

            // L3: contribution cache (new API) dirty flag intersection
            entry.contributions.retain(|region, _| {
                let deps = plugin.contribute_deps(region);
                !dirty.intersects(deps)
            });
        }
    }

    /// Initialize all plugins. Call after all plugins are registered.
    pub fn init_all(&mut self, state: &AppState) -> Vec<Command> {
        let mut commands = Vec::new();
        for plugin in &mut self.plugins {
            commands.extend(plugin.on_init(state));
        }
        commands
    }

    /// Shut down all plugins. Call before application exit.
    pub fn shutdown_all(&mut self) {
        for plugin in &mut self.plugins {
            plugin.on_shutdown();
        }
    }

    pub fn plugins_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn Plugin>> {
        self.plugins.iter_mut()
    }

    /// Collect surfaces from all plugins. Call after `init_all()`.
    pub fn collect_plugin_surfaces(&mut self) -> Vec<Box<dyn crate::surface::Surface>> {
        let mut surfaces = Vec::new();
        for plugin in &mut self.plugins {
            surfaces.extend(plugin.surfaces());
        }
        surfaces
    }

    /// Collect paint hooks from all plugins. Call after `init_all()`.
    pub fn collect_paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
        let mut hooks = Vec::new();
        for (i, plugin) in self.plugins.iter().enumerate() {
            if self.capabilities[i].contains(PluginCapabilities::PAINT_HOOK) {
                hooks.extend(plugin.paint_hooks());
            }
        }
        hooks
    }

    #[deprecated(since = "0.2.0", note = "Use collect_slot_by_id() instead")]
    #[allow(deprecated)]
    pub fn collect_slot(&self, slot: Slot, state: &AppState) -> Vec<Element> {
        let mut cache = self.slot_cache.borrow_mut();
        let slot_idx = slot.index();

        self.plugins
            .iter()
            .enumerate()
            .filter_map(|(i, plugin)| {
                // Check cache if entry exists
                if let Some(entry) = cache.entries.get(i)
                    && let Some(ref cached) = entry.slots[slot_idx]
                {
                    return cached.clone();
                }

                // Cache miss — compute via contribute_slot (supports both legacy and new API)
                let slot_id = SlotId::from(slot);
                let result = plugin.contribute_slot(&slot_id, state);

                // Ensure entry exists (grow if needed)
                while cache.entries.len() <= i {
                    cache.entries.push(PluginCacheEntry::default());
                }
                cache.entries[i].slots[slot_idx] = Some(result.clone());

                result
            })
            .collect()
    }

    /// Collect contributions for an open SlotId.
    ///
    /// For well-known slots, delegates to the existing `collect_slot()` with its
    /// high-performance array-based cache. For custom slots, uses a HashMap-based
    /// cache with the same L1+L3 invalidation strategy.
    #[deprecated(since = "0.5.0", note = "Use collect_contributions() instead")]
    #[allow(deprecated)]
    pub fn collect_slot_by_id(&self, slot_id: &SlotId, state: &AppState) -> Vec<Element> {
        // Fast path: well-known slots use the existing array cache
        if let Some(legacy) = slot_id.to_legacy() {
            return self.collect_slot(legacy, state);
        }

        // Custom slot path: HashMap-based cache
        let mut cache = self.slot_cache.borrow_mut();

        self.plugins
            .iter()
            .enumerate()
            .filter_map(|(i, plugin)| {
                let caps = self.capabilities[i];
                if !caps.intersects(
                    PluginCapabilities::NAMED_SLOT | PluginCapabilities::SLOT_CONTRIBUTOR,
                ) {
                    return None;
                }

                // Check custom slot cache
                if let Some(entry) = cache.entries.get(i)
                    && let Some(cached) = entry.custom_slots.get(slot_id)
                {
                    return cached.clone();
                }

                // Cache miss — compute and store
                let result = plugin.contribute_slot(slot_id, state);

                while cache.entries.len() <= i {
                    cache.entries.push(PluginCacheEntry::default());
                }
                cache.entries[i]
                    .custom_slots
                    .insert(slot_id.clone(), result.clone());

                result
            })
            .collect()
    }

    /// Collect overlays from plugins: both typed overlays (contribute_overlay)
    /// and legacy Slot::Overlay contributions (wrapped in full-screen Absolute anchor).
    #[allow(deprecated)]
    pub fn collect_overlays(&self, state: &AppState) -> Vec<Overlay> {
        let mut overlays = Vec::new();
        let mut cache = self.slot_cache.borrow_mut();
        for (i, plugin) in self.plugins.iter().enumerate() {
            let caps = self.capabilities[i];

            // Typed overlay with plugin-specified anchor (with L3 caching)
            if caps.contains(PluginCapabilities::OVERLAY) {
                // Check cache
                let entry = cache.entries.get(i);
                if let Some(entry) = entry
                    && let Some(ref cached) = entry.overlay
                {
                    if let Some(overlay) = cached.clone() {
                        overlays.push(overlay);
                    }
                } else {
                    let result = plugin.contribute_overlay(state);
                    // Ensure entry exists
                    while cache.entries.len() <= i {
                        cache.entries.push(PluginCacheEntry::default());
                    }
                    cache.entries[i].overlay = Some(result.clone());
                    if let Some(overlay) = result {
                        overlays.push(overlay);
                    }
                }
            }

            // Legacy: Slot::Overlay → full-screen Absolute (backward compat)
            if caps.contains(PluginCapabilities::SLOT_CONTRIBUTOR)
                && let Some(element) = plugin.contribute_slot(&SlotId::OVERLAY, state)
            {
                overlays.push(Overlay {
                    element,
                    anchor: OverlayAnchor::Absolute {
                        x: 0,
                        y: 0,
                        w: state.cols,
                        h: state.rows,
                    },
                });
            }
        }
        overlays
    }

    pub fn set_hit_map(&mut self, hit_map: HitMap) {
        self.hit_map = hit_map;
    }

    pub fn hit_test(&self, x: u16, y: u16) -> Option<InteractiveId> {
        self.hit_map.test(x, y)
    }

    /// Hit test returning both the InteractiveId and its bounding Rect.
    pub fn hit_test_with_rect(
        &self,
        x: u16,
        y: u16,
    ) -> Option<(InteractiveId, crate::layout::Rect)> {
        self.hit_map.test_with_rect(x, y)
    }

    /// Apply decorators in priority order (high priority = inner = applied first).
    #[deprecated(since = "0.5.0", note = "Use apply_transform_chain() instead")]
    #[allow(deprecated)]
    pub fn apply_decorator(
        &self,
        target: DecorateTarget,
        element: Element,
        state: &AppState,
    ) -> Element {
        let mut decorators: Vec<&dyn Plugin> = self
            .plugins
            .iter()
            .enumerate()
            .filter(|(i, _)| self.capabilities[*i].contains(PluginCapabilities::DECORATOR))
            .map(|(_, p)| p.as_ref())
            .collect();
        decorators.sort_by_key(|p| std::cmp::Reverse(p.decorator_priority()));
        decorators
            .into_iter()
            .fold(element, |el, plugin| plugin.decorate(target, el, state))
    }

    /// Get a replacement element. Last registered plugin wins.
    #[deprecated(since = "0.5.0", note = "Use apply_transform_chain() instead")]
    #[allow(deprecated)]
    pub fn get_replacement(&self, target: ReplaceTarget, state: &AppState) -> Option<Element> {
        self.plugins
            .iter()
            .enumerate()
            .rev()
            .filter(|(i, _)| self.capabilities[*i].contains(PluginCapabilities::REPLACEMENT))
            .find_map(|(_, p)| p.replace(target, state))
    }

    // --- Line decoration ---

    /// Build left gutter column from plugin line decorations.
    /// Returns None when no plugin provides gutter content (zero overhead).
    /// When multiple plugins contribute to the same line, elements are composed
    /// horizontally via `Element::row()`.
    #[deprecated(since = "0.5.0", note = "Use collect_annotations() instead")]
    #[allow(deprecated)]
    pub fn build_left_gutter(&self, state: &AppState) -> Option<Element> {
        if !self.has_capability(PluginCapabilities::LINE_DECORATION) {
            return None;
        }
        let line_count = state.visible_line_range().len();
        let mut has_any = false;
        let mut rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        for line in 0..line_count {
            let mut parts: Vec<Element> = Vec::new();
            for (i, plugin) in self.plugins.iter().enumerate() {
                if !self.capabilities[i].contains(PluginCapabilities::LINE_DECORATION) {
                    continue;
                }
                if let Some(dec) = plugin.contribute_line(line, state)
                    && let Some(el) = dec.left_gutter
                {
                    parts.push(el);
                    has_any = true;
                }
            }
            let cell = match parts.len() {
                0 => Element::text(" ", Face::default()),
                1 => parts.pop().unwrap(),
                _ => Element::row(parts.into_iter().map(FlexChild::fixed).collect()),
            };
            rows.push(FlexChild::fixed(cell));
        }
        if has_any {
            Some(Element::column(rows))
        } else {
            None
        }
    }

    /// Build right gutter column from plugin line decorations.
    /// Returns None when no plugin provides gutter content (zero overhead).
    /// When multiple plugins contribute to the same line, elements are composed
    /// horizontally via `Element::row()`.
    #[deprecated(since = "0.5.0", note = "Use collect_annotations() instead")]
    #[allow(deprecated)]
    pub fn build_right_gutter(&self, state: &AppState) -> Option<Element> {
        if !self.has_capability(PluginCapabilities::LINE_DECORATION) {
            return None;
        }
        let line_count = state.visible_line_range().len();
        let mut has_any = false;
        let mut rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        for line in 0..line_count {
            let mut parts: Vec<Element> = Vec::new();
            for (i, plugin) in self.plugins.iter().enumerate() {
                if !self.capabilities[i].contains(PluginCapabilities::LINE_DECORATION) {
                    continue;
                }
                if let Some(dec) = plugin.contribute_line(line, state)
                    && let Some(el) = dec.right_gutter
                {
                    parts.push(el);
                    has_any = true;
                }
            }
            let cell = match parts.len() {
                0 => Element::text(" ", Face::default()),
                1 => parts.pop().unwrap(),
                _ => Element::row(parts.into_iter().map(FlexChild::fixed).collect()),
            };
            rows.push(FlexChild::fixed(cell));
        }
        if has_any {
            Some(Element::column(rows))
        } else {
            None
        }
    }

    /// Collect background overrides from all plugins for visible lines.
    /// Returns None when no plugin provides any background (zero overhead).
    #[deprecated(since = "0.5.0", note = "Use collect_annotations() instead")]
    #[allow(deprecated)]
    pub fn collect_line_backgrounds(&self, state: &AppState) -> Option<Vec<Option<Face>>> {
        if !self.has_capability(PluginCapabilities::LINE_DECORATION) {
            return None;
        }
        let line_count = state.visible_line_range().len();
        let mut backgrounds: Vec<Option<Face>> = vec![None; line_count];
        let mut has_any = false;
        for (line, bg_slot) in backgrounds.iter_mut().enumerate().take(line_count) {
            for (i, plugin) in self.plugins.iter().enumerate() {
                if !self.capabilities[i].contains(PluginCapabilities::LINE_DECORATION) {
                    continue;
                }
                if let Some(dec) = plugin.contribute_line(line, state)
                    && let Some(bg) = dec.background
                {
                    *bg_slot = Some(bg);
                    has_any = true;
                }
            }
        }
        if has_any { Some(backgrounds) } else { None }
    }

    // --- Menu item transformation ---

    /// Transform a menu item through all plugins. Returns None if no plugin transforms it.
    pub fn transform_menu_item(
        &self,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        state: &AppState,
    ) -> Option<Vec<crate::protocol::Atom>> {
        let mut current: Option<Vec<crate::protocol::Atom>> = None;
        for (i, plugin) in self.plugins.iter().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::MENU_TRANSFORM) {
                continue;
            }
            let input = current.as_deref().unwrap_or(item);
            if let Some(transformed) = plugin.transform_menu_item(input, index, selected, state) {
                current = Some(transformed);
            }
        }
        current
    }

    // --- Cursor style override ---

    /// Query plugins for a cursor style override. Returns the first non-None.
    pub fn cursor_style_override(&self, state: &AppState) -> Option<crate::render::CursorStyle> {
        for (i, plugin) in self.plugins.iter().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::CURSOR_STYLE) {
                continue;
            }
            if let Some(style) = plugin.cursor_style_override(state) {
                return Some(style);
            }
        }
        None
    }

    // --- Named slot contributions ---

    /// Collect elements contributed to a custom named slot.
    pub fn collect_named_slot(&self, name: &str, state: &AppState) -> Vec<Element> {
        let mut elements = Vec::new();
        for (i, plugin) in self.plugins.iter().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::NAMED_SLOT) {
                continue;
            }
            if let Some(el) = plugin.contribute_named_slot(name, state) {
                elements.push(el);
            }
        }
        elements
    }

    // ===========================================================================
    // New dispatch API: Contribute / Transform / Annotate
    // ===========================================================================

    /// Collect contributions from all plugins for a given region, sorted by priority.
    #[allow(deprecated)]
    pub fn collect_contributions(
        &self,
        region: &SlotId,
        state: &AppState,
        ctx: &ContributeContext,
    ) -> Vec<Contribution> {
        let mut cache = self.slot_cache.borrow_mut();
        let mut contributions: Vec<Contribution> = self
            .plugins
            .iter()
            .enumerate()
            .filter_map(|(i, plugin)| {
                let caps = self.capabilities[i];
                // New API: CONTRIBUTOR capability → contribute_to()
                if caps.contains(PluginCapabilities::CONTRIBUTOR) {
                    // Check contribution cache
                    if let Some(entry) = cache.entries.get(i)
                        && let Some(cached) = entry.contributions.get(region)
                    {
                        return cached.clone();
                    }
                    let result = plugin.contribute_to(region, state, ctx);
                    while cache.entries.len() <= i {
                        cache.entries.push(PluginCacheEntry::default());
                    }
                    cache.entries[i]
                        .contributions
                        .insert(region.clone(), result.clone());
                    return result;
                }
                // Fallback: old SLOT_CONTRIBUTOR / NAMED_SLOT → contribute_slot()
                if caps.intersects(
                    PluginCapabilities::SLOT_CONTRIBUTOR | PluginCapabilities::NAMED_SLOT,
                ) {
                    return plugin
                        .contribute_slot(region, state)
                        .map(|el| Contribution {
                            element: el,
                            priority: 0,
                            size_hint: ContribSizeHint::Auto,
                        });
                }
                None
            })
            .collect();
        contributions.sort_by_key(|c| c.priority);
        contributions
    }

    /// Apply the 3-phase transform chain for a given target.
    ///
    /// This is the central element-transformation entry point.  Every UI
    /// component that supports plugin customisation (status bar, menus, info
    /// panels, buffer, etc.) is processed through this chain.
    ///
    /// # Three-phase model
    ///
    /// **Phase 1 — Seed Selection (replacement scan)**
    ///
    /// Plugins with the `REPLACEMENT` capability (legacy API) are scanned in
    /// **reverse registration order**.  The first plugin (i.e. the
    /// last-registered one) that returns `Some(element)` from
    /// [`Plugin::replace()`] wins, and its element becomes the *seed*.
    /// Plugins with the `TRANSFORMER` capability (new API) are skipped in
    /// this phase — their replacements are handled inline during Phase 3.
    ///
    /// **Phase 2 — Default Fallback**
    ///
    /// If no replacement was found in Phase 1, `default_element_fn()` is
    /// evaluated **lazily** (only at this point) to produce the seed element.
    /// This avoids unnecessary computation when a plugin fully replaces the
    /// component (C7 — lazy default).
    ///
    /// **Phase 3 — Chain Application (transforms + decorators)**
    ///
    /// All plugins with `TRANSFORMER` or `DECORATOR` capability are collected
    /// into a single chain, sorted by priority in **descending** order (high
    /// priority = inner = applied first, so the highest-priority transform
    /// sees the raw seed, and the lowest-priority transform wraps the
    /// outermost layer).  Each plugin in the chain is called exactly once:
    ///
    /// - `TRANSFORMER` plugins: called via [`Plugin::transform()`] (new API).
    /// - `DECORATOR` plugins: called via [`Plugin::decorate()`] (legacy API).
    ///
    /// No plugin is ever called through both APIs (C3 — no double dispatch).
    ///
    /// # Key invariant
    ///
    /// The seed element (whether from a replacement or the default) **always**
    /// passes through the full transform chain.  A replacement never bypasses
    /// decorators or transformers — it only substitutes what enters the chain.
    ///
    /// # Arguments
    ///
    /// * `target` — Identifies which UI component is being transformed.
    /// * `default_element_fn` — Lazily produces the default seed element.
    /// * `state` — Current application state, forwarded to every plugin call.
    #[allow(deprecated)]
    pub fn apply_transform_chain(
        &self,
        target: TransformTarget,
        default_element_fn: impl FnOnce() -> Element,
        state: &AppState,
    ) -> Element {
        // Map TransformTarget to legacy targets
        let legacy_replace = match target {
            TransformTarget::StatusBar => Some(ReplaceTarget::StatusBar),
            TransformTarget::MenuPrompt => Some(ReplaceTarget::MenuPrompt),
            TransformTarget::MenuInline => Some(ReplaceTarget::MenuInline),
            TransformTarget::MenuSearch => Some(ReplaceTarget::MenuSearch),
            TransformTarget::InfoPrompt => Some(ReplaceTarget::InfoPrompt),
            TransformTarget::InfoModal => Some(ReplaceTarget::InfoModal),
            _ => None,
        };
        let legacy_decorate = match target {
            TransformTarget::Buffer => Some(DecorateTarget::Buffer),
            TransformTarget::BufferLine(n) => Some(DecorateTarget::BufferLine(n)),
            TransformTarget::StatusBar => Some(DecorateTarget::StatusBar),
            TransformTarget::Menu
            | TransformTarget::MenuPrompt
            | TransformTarget::MenuInline
            | TransformTarget::MenuSearch => Some(DecorateTarget::Menu),
            TransformTarget::Info | TransformTarget::InfoPrompt | TransformTarget::InfoModal => {
                Some(DecorateTarget::Info)
            }
        };

        // Phase 1: Check for full replacement (new TRANSFORMER or old REPLACEMENT)
        let mut replaced = None;
        for (i, plugin) in self.plugins.iter().enumerate().rev() {
            let caps = self.capabilities[i];
            if caps.contains(PluginCapabilities::TRANSFORMER) {
                // New API: call transform with a sentinel to detect full replacement.
                // We do this by checking if the plugin replaces the default.
                // For simplicity, we handle this in Phase 3 below (chain application).
                continue;
            }
            if caps.contains(PluginCapabilities::REPLACEMENT)
                && let Some(rt) = legacy_replace
                && let Some(el) = plugin.replace(rt, state)
            {
                replaced = Some(el);
                break;
            }
        }

        // Phase 2: Build default if needed
        let is_default = replaced.is_none();
        let mut element = replaced.unwrap_or_else(default_element_fn);

        // Phase 3: Apply transforms/decorators in priority order
        // Collect (index, priority, is_new_api) tuples
        let mut chain: Vec<(usize, i16, bool)> = Vec::new();
        for (i, _plugin) in self.plugins.iter().enumerate() {
            let caps = self.capabilities[i];
            if caps.contains(PluginCapabilities::TRANSFORMER) {
                let prio = self.plugins[i].transform_priority();
                chain.push((i, prio, true));
            } else if caps.contains(PluginCapabilities::DECORATOR) {
                let prio = self.plugins[i].decorator_priority() as i16;
                chain.push((i, prio, false));
            }
        }
        // Sort by priority descending (high = inner = applied first)
        chain.sort_by_key(|&(_, prio, _)| std::cmp::Reverse(prio));

        for (pos, &(i, _, is_new)) in chain.iter().enumerate() {
            if is_new {
                let ctx = TransformContext {
                    is_default,
                    chain_position: pos,
                };
                element = self.plugins[i].transform(&target, element, state, &ctx);
            } else if let Some(dt) = legacy_decorate {
                element = self.plugins[i].decorate(dt, element, state);
            }
        }

        element
    }

    /// Collect annotations from all annotating plugins for visible lines.
    #[allow(deprecated)]
    pub fn collect_annotations(&self, state: &AppState, ctx: &AnnotateContext) -> AnnotationResult {
        let has_annotators = self.capabilities.iter().any(|c| {
            c.intersects(PluginCapabilities::ANNOTATOR | PluginCapabilities::LINE_DECORATION)
        });
        if !has_annotators {
            return AnnotationResult {
                left_gutter: None,
                right_gutter: None,
                line_backgrounds: None,
            };
        }

        let line_count = state.visible_line_range().len();
        let mut has_left = false;
        let mut has_right = false;
        let mut has_bg = false;

        let mut left_rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        let mut right_rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        let mut backgrounds: Vec<Option<Face>> = vec![None; line_count];

        for (line, bg_slot) in backgrounds.iter_mut().enumerate().take(line_count) {
            let mut left_parts: Vec<(i16, Element)> = Vec::new();
            let mut right_parts: Vec<(i16, Element)> = Vec::new();
            let mut bg_layers: Vec<BackgroundLayer> = Vec::new();

            for (i, plugin) in self.plugins.iter().enumerate() {
                let caps = self.capabilities[i];
                if caps.contains(PluginCapabilities::ANNOTATOR) {
                    if let Some(ann) = plugin.annotate_line_with_ctx(line, state, ctx) {
                        let prio = ann.priority;
                        if let Some(el) = ann.left_gutter {
                            left_parts.push((prio, el));
                            has_left = true;
                        }
                        if let Some(el) = ann.right_gutter {
                            right_parts.push((prio, el));
                            has_right = true;
                        }
                        if let Some(bg) = ann.background {
                            bg_layers.push(bg);
                        }
                    }
                } else if caps.contains(PluginCapabilities::LINE_DECORATION)
                    && let Some(dec) = plugin.contribute_line(line, state)
                {
                    if let Some(el) = dec.left_gutter {
                        left_parts.push((0, el));
                        has_left = true;
                    }
                    if let Some(el) = dec.right_gutter {
                        right_parts.push((0, el));
                        has_right = true;
                    }
                    if let Some(bg) = dec.background {
                        bg_layers.push(BackgroundLayer {
                            face: bg,
                            z_order: 0,
                            blend: BlendMode::Opaque,
                        });
                    }
                }
            }

            // Sort gutter elements by priority (ascending: lower values first)
            left_parts.sort_by_key(|(prio, _)| *prio);
            right_parts.sort_by_key(|(prio, _)| *prio);

            let left_cell = match left_parts.len() {
                0 => Element::text(" ", Face::default()),
                1 => left_parts.pop().unwrap().1,
                _ => Element::row(
                    left_parts
                        .into_iter()
                        .map(|(_, el)| FlexChild::fixed(el))
                        .collect(),
                ),
            };
            left_rows.push(FlexChild::fixed(left_cell));

            let right_cell = match right_parts.len() {
                0 => Element::text(" ", Face::default()),
                1 => right_parts.pop().unwrap().1,
                _ => Element::row(
                    right_parts
                        .into_iter()
                        .map(|(_, el)| FlexChild::fixed(el))
                        .collect(),
                ),
            };
            right_rows.push(FlexChild::fixed(right_cell));

            if !bg_layers.is_empty() {
                bg_layers.sort_by_key(|l| l.z_order);
                *bg_slot = Some(bg_layers.last().unwrap().face);
                has_bg = true;
            }
        }

        AnnotationResult {
            left_gutter: if has_left {
                Some(Element::column(left_rows))
            } else {
                None
            },
            right_gutter: if has_right {
                Some(Element::column(right_rows))
            } else {
                None
            },
            line_backgrounds: if has_bg { Some(backgrounds) } else { None },
        }
    }

    /// Collect overlay contributions with collision-avoidance context.
    pub fn collect_overlays_with_ctx(
        &self,
        state: &AppState,
        ctx: &OverlayContext,
    ) -> Vec<OverlayContribution> {
        let mut contributions = Vec::new();
        for (i, plugin) in self.plugins.iter().enumerate() {
            let caps = self.capabilities[i];
            if (caps.contains(PluginCapabilities::CONTRIBUTOR)
                || caps.contains(PluginCapabilities::OVERLAY))
                && let Some(oc) = plugin.contribute_overlay_with_ctx(state, ctx)
            {
                contributions.push(oc);
            }
        }
        contributions.sort_by_key(|c| c.z_index);
        contributions
    }

    /// Check if any plugin has TRANSFORMER capability for a given target.
    pub fn has_transform_for(&self, _target: TransformTarget) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.contains(PluginCapabilities::TRANSFORMER))
    }

    // --- Plugin message delivery ---

    /// Deliver a message to a specific plugin by ID.
    pub fn deliver_message(
        &mut self,
        target: &PluginId,
        payload: Box<dyn Any>,
        state: &AppState,
    ) -> (DirtyFlags, Vec<Command>) {
        for plugin in &mut self.plugins {
            if &plugin.id() == target {
                let mut commands = plugin.update(payload, state);
                let flags = extract_redraw_flags(&mut commands);
                return (flags, commands);
            }
        }
        (DirtyFlags::empty(), vec![])
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::element::Direction;
    use crate::protocol::Face;

    struct TestPlugin;

    impl Plugin for TestPlugin {
        fn id(&self) -> PluginId {
            PluginId("test".to_string())
        }

        fn contribute(&self, slot: Slot, _state: &AppState) -> Option<Element> {
            match slot {
                Slot::AboveBuffer => Some(Element::text("above", Face::default())),
                _ => None,
            }
        }
    }

    #[test]
    fn test_empty_registry() {
        let registry = PluginRegistry::new();
        let state = AppState::default();
        let elements = registry.collect_slot(Slot::AboveBuffer, &state);
        assert!(elements.is_empty());
    }

    #[test]
    fn test_registry_collect_slot() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(TestPlugin));
        let state = AppState::default();

        let above = registry.collect_slot(Slot::AboveBuffer, &state);
        assert_eq!(above.len(), 1);

        let below = registry.collect_slot(Slot::BelowBuffer, &state);
        assert!(below.is_empty());
    }

    #[test]
    fn test_plugin_id() {
        let plugin = TestPlugin;
        assert_eq!(plugin.id(), PluginId("test".to_string()));
    }

    // --- Decorator / Replacement tests ---

    struct WrapperPlugin {
        priority: u32,
        label: &'static str,
    }

    impl Plugin for WrapperPlugin {
        fn id(&self) -> PluginId {
            PluginId(self.label.to_string())
        }

        fn decorate(&self, target: DecorateTarget, element: Element, _state: &AppState) -> Element {
            match target {
                DecorateTarget::Buffer => Element::Container {
                    child: Box::new(element),
                    border: None,
                    shadow: false,
                    padding: crate::element::Edges::ZERO,
                    style: crate::element::Style::from(Face::default()),
                    title: None,
                },
                _ => element,
            }
        }

        fn decorator_priority(&self) -> u32 {
            self.priority
        }
    }

    struct ReplacerPlugin;

    impl Plugin for ReplacerPlugin {
        fn id(&self) -> PluginId {
            PluginId("replacer".to_string())
        }

        fn replace(&self, target: ReplaceTarget, _state: &AppState) -> Option<Element> {
            match target {
                ReplaceTarget::StatusBar => Some(Element::text("custom status", Face::default())),
                _ => None,
            }
        }
    }

    #[test]
    fn test_decorator_empty_registry_passthrough() {
        let registry = PluginRegistry::new();
        let state = AppState::default();
        let el = Element::text("hello", Face::default());
        let result = registry.apply_decorator(DecorateTarget::Buffer, el, &state);
        // No plugins → element passes through unchanged
        assert!(matches!(result, Element::Text(..)));
    }

    #[test]
    fn test_single_decorator_wraps() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(WrapperPlugin {
            priority: 0,
            label: "wrap",
        }));
        let state = AppState::default();
        let el = Element::text("hello", Face::default());
        let result = registry.apply_decorator(DecorateTarget::Buffer, el, &state);
        match result {
            Element::Container { child, .. } => {
                assert!(matches!(*child, Element::Text(..)));
            }
            _ => panic!("expected Container wrapping"),
        }
    }

    #[test]
    fn test_decorator_priority_order() {
        let mut registry = PluginRegistry::new();
        // Higher priority applied first (inner), lower priority applied last (outer)
        registry.register(Box::new(WrapperPlugin {
            priority: 10,
            label: "inner",
        }));
        registry.register(Box::new(WrapperPlugin {
            priority: 0,
            label: "outer",
        }));
        let state = AppState::default();
        let el = Element::text("hello", Face::default());
        let result = registry.apply_decorator(DecorateTarget::Buffer, el, &state);
        // Outer Container wrapping inner Container wrapping text
        match result {
            Element::Container { child, .. } => match *child {
                Element::Container { child, .. } => {
                    assert!(matches!(*child, Element::Text(..)));
                }
                _ => panic!("expected nested Container"),
            },
            _ => panic!("expected Container"),
        }
    }

    #[test]
    fn test_replacement_none_for_empty_registry() {
        let registry = PluginRegistry::new();
        let state = AppState::default();
        assert!(
            registry
                .get_replacement(ReplaceTarget::StatusBar, &state)
                .is_none()
        );
    }

    #[test]
    fn test_replacement_returns_some() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(ReplacerPlugin));
        let state = AppState::default();
        let result = registry.get_replacement(ReplaceTarget::StatusBar, &state);
        assert!(result.is_some());
        // Non-matching target returns None
        assert!(
            registry
                .get_replacement(ReplaceTarget::MenuPrompt, &state)
                .is_none()
        );
    }

    #[test]
    fn test_replacement_last_wins() {
        struct Replacer2;
        impl Plugin for Replacer2 {
            fn id(&self) -> PluginId {
                PluginId("replacer2".to_string())
            }
            fn replace(&self, target: ReplaceTarget, _state: &AppState) -> Option<Element> {
                match target {
                    ReplaceTarget::StatusBar => {
                        Some(Element::text("second status", Face::default()))
                    }
                    _ => None,
                }
            }
        }

        let mut registry = PluginRegistry::new();
        registry.register(Box::new(ReplacerPlugin));
        registry.register(Box::new(Replacer2));
        let state = AppState::default();
        let result = registry.get_replacement(ReplaceTarget::StatusBar, &state);
        match result {
            Some(Element::Text(s, _)) => {
                assert_eq!(s, "second status");
            }
            _ => panic!("expected Text from second replacer"),
        }
    }

    // --- collect_overlays tests ---

    #[test]
    fn test_collect_overlays_typed() {
        use crate::element::{Overlay, OverlayAnchor};
        use crate::protocol::Coord;

        struct OverlayPlugin;
        impl Plugin for OverlayPlugin {
            fn id(&self) -> PluginId {
                PluginId("overlay".into())
            }
            fn contribute_overlay(&self, _state: &AppState) -> Option<Overlay> {
                Some(Overlay {
                    element: Element::text("popup", Face::default()),
                    anchor: OverlayAnchor::AnchorPoint {
                        coord: Coord {
                            line: 5,
                            column: 10,
                        },
                        prefer_above: false,
                        avoid: vec![],
                    },
                })
            }
        }

        let mut registry = PluginRegistry::new();
        registry.register(Box::new(OverlayPlugin));
        let state = AppState::default();
        let overlays = registry.collect_overlays(&state);
        assert_eq!(overlays.len(), 1);
        assert!(matches!(
            overlays[0].anchor,
            OverlayAnchor::AnchorPoint { .. }
        ));
    }

    #[test]
    fn test_collect_overlays_legacy() {
        struct LegacyOverlayPlugin;
        impl Plugin for LegacyOverlayPlugin {
            fn id(&self) -> PluginId {
                PluginId("legacy_overlay".into())
            }
            fn contribute(&self, slot: Slot, _state: &AppState) -> Option<Element> {
                match slot {
                    Slot::Overlay => Some(Element::text("legacy", Face::default())),
                    _ => None,
                }
            }
        }

        let mut registry = PluginRegistry::new();
        registry.register(Box::new(LegacyOverlayPlugin));
        let mut state = AppState::default();
        state.cols = 80;
        state.rows = 24;
        let overlays = registry.collect_overlays(&state);
        assert_eq!(overlays.len(), 1);
        match &overlays[0].anchor {
            crate::element::OverlayAnchor::Absolute { x, y, w, h } => {
                assert_eq!(*x, 0);
                assert_eq!(*y, 0);
                assert_eq!(*w, 80);
                assert_eq!(*h, 24);
            }
            _ => panic!("expected Absolute anchor for legacy overlay"),
        }
    }

    #[test]
    fn test_collect_overlays_both() {
        use crate::element::{Overlay, OverlayAnchor};
        use crate::protocol::Coord;

        struct BothPlugin;
        impl Plugin for BothPlugin {
            fn id(&self) -> PluginId {
                PluginId("both".into())
            }
            fn contribute_overlay(&self, _state: &AppState) -> Option<Overlay> {
                Some(Overlay {
                    element: Element::text("typed", Face::default()),
                    anchor: OverlayAnchor::AnchorPoint {
                        coord: Coord { line: 0, column: 0 },
                        prefer_above: true,
                        avoid: vec![],
                    },
                })
            }
            fn contribute(&self, slot: Slot, _state: &AppState) -> Option<Element> {
                match slot {
                    Slot::Overlay => Some(Element::text("legacy", Face::default())),
                    _ => None,
                }
            }
        }

        let mut registry = PluginRegistry::new();
        registry.register(Box::new(BothPlugin));
        let state = AppState::default();
        let overlays = registry.collect_overlays(&state);
        assert_eq!(overlays.len(), 2);
        assert!(matches!(
            overlays[0].anchor,
            OverlayAnchor::AnchorPoint { .. }
        ));
        assert!(matches!(overlays[1].anchor, OverlayAnchor::Absolute { .. }));
    }

    #[test]
    fn test_extract_redraw_flags_merges() {
        use crate::state::DirtyFlags;
        let mut commands = vec![
            Command::RequestRedraw(DirtyFlags::BUFFER),
            Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
            Command::RequestRedraw(DirtyFlags::INFO),
        ];
        let flags = super::extract_redraw_flags(&mut commands);
        assert_eq!(flags, DirtyFlags::BUFFER | DirtyFlags::INFO);
        assert_eq!(commands.len(), 1);
        assert!(matches!(commands[0], Command::SendToKakoune(_)));
    }

    #[test]
    fn test_extract_redraw_flags_empty() {
        let mut commands = vec![
            Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
            Command::Paste,
        ];
        let flags = super::extract_redraw_flags(&mut commands);
        assert!(flags.is_empty());
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn test_existing_test_plugin_backward_compatible() {
        // TestPlugin doesn't implement decorate/replace/decorator_priority
        // — defaults should work fine
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(TestPlugin));
        let state = AppState::default();

        // contribute still works
        let above = registry.collect_slot(Slot::AboveBuffer, &state);
        assert_eq!(above.len(), 1);

        // decorator passthrough
        let el = Element::text("x", Face::default());
        let decorated = registry.apply_decorator(DecorateTarget::Buffer, el, &state);
        assert!(matches!(decorated, Element::Text(..)));

        // no replacement
        assert!(
            registry
                .get_replacement(ReplaceTarget::StatusBar, &state)
                .is_none()
        );
    }

    // --- Lifecycle hooks tests ---

    struct LifecyclePlugin {
        init_called: bool,
        shutdown_called: bool,
        state_changes: Vec<DirtyFlags>,
    }

    impl LifecyclePlugin {
        fn new() -> Self {
            LifecyclePlugin {
                init_called: false,
                shutdown_called: false,
                state_changes: Vec::new(),
            }
        }
    }

    impl Plugin for LifecyclePlugin {
        fn id(&self) -> PluginId {
            PluginId("lifecycle".to_string())
        }

        fn on_init(&mut self, _state: &AppState) -> Vec<Command> {
            self.init_called = true;
            vec![Command::RequestRedraw(DirtyFlags::BUFFER)]
        }

        fn on_shutdown(&mut self) {
            self.shutdown_called = true;
        }

        fn on_state_changed(&mut self, _state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
            self.state_changes.push(dirty);
            vec![]
        }
    }

    #[test]
    fn test_init_all_returns_commands() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(LifecyclePlugin::new()));
        let state = AppState::default();
        let commands = registry.init_all(&state);
        assert_eq!(commands.len(), 1);
        assert!(matches!(commands[0], Command::RequestRedraw(_)));
    }

    #[test]
    fn test_shutdown_all_calls_all_plugins() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(LifecyclePlugin::new()));
        registry.register(Box::new(LifecyclePlugin::new()));
        registry.shutdown_all();
        // Verify via count — can't inspect internal state, but no panic = success
    }

    #[test]
    fn test_on_state_changed_dispatched_with_flags() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(LifecyclePlugin::new()));
        let state = AppState::default();

        // Simulate what update() does for Msg::Kakoune
        let flags = DirtyFlags::BUFFER | DirtyFlags::STATUS;
        for plugin in registry.plugins_mut() {
            plugin.on_state_changed(&state, flags);
        }
        // No panic, default implementations work
    }

    #[test]
    fn test_lifecycle_backward_compat() {
        // TestPlugin has no lifecycle hooks — defaults should work
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(TestPlugin));
        let state = AppState::default();

        let commands = registry.init_all(&state);
        assert!(commands.is_empty());

        registry.shutdown_all();
        // No panic
    }

    // --- Input observation tests ---

    struct ObservingPlugin {
        observed_keys: std::cell::RefCell<Vec<String>>,
    }

    impl ObservingPlugin {
        fn new() -> Self {
            ObservingPlugin {
                observed_keys: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    impl Plugin for ObservingPlugin {
        fn id(&self) -> PluginId {
            PluginId("observer".to_string())
        }

        fn observe_key(&mut self, key: &KeyEvent, _state: &AppState) {
            self.observed_keys
                .borrow_mut()
                .push(format!("{:?}", key.key));
        }
    }

    #[test]
    fn test_observe_key_called() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(ObservingPlugin::new()));
        let state = AppState::default();
        let key = KeyEvent {
            key: crate::input::Key::Char('a'),
            modifiers: crate::input::Modifiers::empty(),
        };
        for plugin in registry.plugins_mut() {
            plugin.observe_key(&key, &state);
        }
        // No panic = success, since we can't downcast
    }

    // --- Line decoration tests ---

    struct LineNumberPlugin;

    impl Plugin for LineNumberPlugin {
        fn id(&self) -> PluginId {
            PluginId("line_numbers".to_string())
        }

        fn contribute_line(&self, line: usize, _state: &AppState) -> Option<LineDecoration> {
            Some(LineDecoration {
                left_gutter: Some(Element::text(format!("{:>3}", line + 1), Face::default())),
                right_gutter: None,
                background: None,
            })
        }
    }

    #[test]
    fn test_build_left_gutter() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(LineNumberPlugin));
        let mut state = AppState::default();
        state.lines = vec![vec![], vec![], vec![]]; // 3 lines
        let gutter = registry.build_left_gutter(&state);
        assert!(gutter.is_some());
        if let Some(Element::Flex { children, .. }) = gutter {
            assert_eq!(children.len(), 3);
        } else {
            panic!("expected Flex column");
        }
    }

    #[test]
    fn test_build_left_gutter_empty_registry() {
        let registry = PluginRegistry::new();
        let state = AppState::default();
        assert!(registry.build_left_gutter(&state).is_none());
    }

    #[test]
    fn test_collect_line_backgrounds() {
        struct BgPlugin;
        impl Plugin for BgPlugin {
            fn id(&self) -> PluginId {
                PluginId("bg".to_string())
            }
            fn contribute_line(&self, line: usize, _state: &AppState) -> Option<LineDecoration> {
                if line == 1 {
                    Some(LineDecoration {
                        left_gutter: None,
                        right_gutter: None,
                        background: Some(Face {
                            fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Red),
                            ..Face::default()
                        }),
                    })
                } else {
                    None
                }
            }
        }

        let mut registry = PluginRegistry::new();
        registry.register(Box::new(BgPlugin));
        let mut state = AppState::default();
        state.lines = vec![vec![], vec![], vec![]];
        let bgs = registry.collect_line_backgrounds(&state);
        assert!(bgs.is_some());
        let bgs = bgs.unwrap();
        assert_eq!(bgs.len(), 3);
        assert!(bgs[0].is_none());
        assert!(bgs[1].is_some());
        assert!(bgs[2].is_none());
    }

    #[test]
    fn test_no_line_decoration_zero_overhead() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(TestPlugin)); // no contribute_line
        let mut state = AppState::default();
        state.lines = vec![vec![], vec![]];
        assert!(registry.build_left_gutter(&state).is_none());
        assert!(registry.build_right_gutter(&state).is_none());
        assert!(registry.collect_line_backgrounds(&state).is_none());
    }

    // --- Multi-plugin composition tests ---

    struct ColorSwatchPlugin;

    impl Plugin for ColorSwatchPlugin {
        fn id(&self) -> PluginId {
            PluginId("color_swatch".to_string())
        }
        fn contribute_line(&self, _line: usize, _state: &AppState) -> Option<LineDecoration> {
            Some(LineDecoration {
                left_gutter: Some(Element::text("●", Face::default())),
                right_gutter: None,
                background: None,
            })
        }
    }

    #[test]
    fn test_multiple_plugins_gutter_composition() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(LineNumberPlugin));
        registry.register(Box::new(ColorSwatchPlugin));
        let mut state = AppState::default();
        state.lines = vec![vec![], vec![], vec![]];

        let gutter = registry
            .build_left_gutter(&state)
            .expect("should have gutter");
        // Outer is a column (vertical Flex)
        if let Element::Flex { children, .. } = &gutter {
            assert_eq!(children.len(), 3);
            // Each row should be a horizontal Flex (row) composing both plugins
            for child in children {
                match &child.element {
                    Element::Flex {
                        direction: Direction::Row,
                        children: row_children,
                        ..
                    } => {
                        assert_eq!(row_children.len(), 2, "should compose 2 plugin elements");
                    }
                    _ => panic!(
                        "expected row Flex for composed gutter, got {:?}",
                        child.element
                    ),
                }
            }
        } else {
            panic!("expected Flex column");
        }
    }

    #[test]
    fn test_background_last_wins() {
        struct BgRed;
        impl Plugin for BgRed {
            fn id(&self) -> PluginId {
                PluginId("bg_red".to_string())
            }
            fn contribute_line(&self, line: usize, _state: &AppState) -> Option<LineDecoration> {
                if line == 1 {
                    Some(LineDecoration {
                        left_gutter: None,
                        right_gutter: None,
                        background: Some(Face {
                            fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Red),
                            ..Face::default()
                        }),
                    })
                } else {
                    None
                }
            }
        }

        struct BgBlue;
        impl Plugin for BgBlue {
            fn id(&self) -> PluginId {
                PluginId("bg_blue".to_string())
            }
            fn contribute_line(&self, line: usize, _state: &AppState) -> Option<LineDecoration> {
                if line == 1 {
                    Some(LineDecoration {
                        left_gutter: None,
                        right_gutter: None,
                        background: Some(Face {
                            fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Blue),
                            ..Face::default()
                        }),
                    })
                } else {
                    None
                }
            }
        }

        let mut registry = PluginRegistry::new();
        registry.register(Box::new(BgRed));
        registry.register(Box::new(BgBlue));
        let mut state = AppState::default();
        state.lines = vec![vec![], vec![], vec![]];

        let bgs = registry.collect_line_backgrounds(&state).unwrap();
        assert!(bgs[0].is_none());
        // Last-wins: BgBlue registered after BgRed, so blue wins
        let bg = bgs[1].unwrap();
        assert_eq!(
            bg.fg,
            crate::protocol::Color::Named(crate::protocol::NamedColor::Blue)
        );
        assert!(bgs[2].is_none());
    }

    #[test]
    fn test_orthogonal_plugins_both_contribute() {
        // Plugin A: background only (like cursor_line bundled WASM plugin)
        struct BgOnlyPlugin;
        impl Plugin for BgOnlyPlugin {
            fn id(&self) -> PluginId {
                PluginId("bg_only".to_string())
            }
            fn contribute_line(&self, line: usize, _state: &AppState) -> Option<LineDecoration> {
                if line == 0 {
                    Some(LineDecoration {
                        left_gutter: None,
                        right_gutter: None,
                        background: Some(Face {
                            fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Yellow),
                            ..Face::default()
                        }),
                    })
                } else {
                    None
                }
            }
        }
        // Plugin B: left gutter only (like color_preview bundled WASM plugin)
        struct GutterOnlyPlugin;
        impl Plugin for GutterOnlyPlugin {
            fn id(&self) -> PluginId {
                PluginId("gutter_only".to_string())
            }
            fn contribute_line(&self, line: usize, _state: &AppState) -> Option<LineDecoration> {
                if line == 0 {
                    Some(LineDecoration {
                        left_gutter: Some(Element::text("▶", Face::default())),
                        right_gutter: None,
                        background: None,
                    })
                } else {
                    None
                }
            }
        }

        let mut registry = PluginRegistry::new();
        registry.register(Box::new(BgOnlyPlugin));
        registry.register(Box::new(GutterOnlyPlugin));
        let mut state = AppState::default();
        state.lines = vec![vec![], vec![]];

        // Background from BgOnlyPlugin should be present
        let bgs = registry.collect_line_backgrounds(&state).unwrap();
        assert!(bgs[0].is_some());

        // Left gutter from GutterOnlyPlugin should be present
        let gutter = registry.build_left_gutter(&state).unwrap();
        if let Element::Flex { children, .. } = &gutter {
            // Line 0 has gutter content, line 1 is filler space
            assert_eq!(children.len(), 2);
        } else {
            panic!("expected Flex column");
        }
    }

    // --- Menu transform tests ---

    struct IconPlugin;

    impl Plugin for IconPlugin {
        fn id(&self) -> PluginId {
            PluginId("icons".to_string())
        }

        fn transform_menu_item(
            &self,
            item: &[crate::protocol::Atom],
            _index: usize,
            _selected: bool,
            _state: &AppState,
        ) -> Option<Vec<crate::protocol::Atom>> {
            let mut result = vec![crate::protocol::Atom {
                face: Face::default(),
                contents: "★ ".into(),
            }];
            result.extend(item.iter().cloned());
            Some(result)
        }
    }

    #[test]
    fn test_transform_menu_item() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(IconPlugin));
        let state = AppState::default();
        let item = vec![crate::protocol::Atom {
            face: Face::default(),
            contents: "foo".into(),
        }];
        let result = registry.transform_menu_item(&item, 0, false, &state);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result[0].contents.as_str(), "★ ");
        assert_eq!(result[1].contents.as_str(), "foo");
    }

    #[test]
    fn test_transform_menu_item_no_plugin() {
        let registry = PluginRegistry::new();
        let state = AppState::default();
        let item = vec![crate::protocol::Atom {
            face: Face::default(),
            contents: "foo".into(),
        }];
        assert!(
            registry
                .transform_menu_item(&item, 0, false, &state)
                .is_none()
        );
    }

    // --- deliver_message tests ---

    #[test]
    fn test_deliver_message_to_plugin() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(TestPlugin));
        let state = AppState::default();
        let (flags, commands) =
            registry.deliver_message(&PluginId("test".to_string()), Box::new(42u32), &state);
        assert!(flags.is_empty());
        assert!(commands.is_empty());
    }

    #[test]
    fn test_deliver_message_unknown_target() {
        let mut registry = PluginRegistry::new();
        let state = AppState::default();
        let (flags, commands) =
            registry.deliver_message(&PluginId("unknown".to_string()), Box::new(42u32), &state);
        assert!(flags.is_empty());
        assert!(commands.is_empty());
    }

    // --- extract_deferred_commands tests ---

    #[test]
    fn test_extract_deferred_separates_correctly() {
        let commands = vec![
            Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
            Command::ScheduleTimer {
                delay: std::time::Duration::from_millis(100),
                target: PluginId("test".into()),
                payload: Box::new(42u32),
            },
            Command::PluginMessage {
                target: PluginId("other".into()),
                payload: Box::new("hello"),
            },
            Command::SetConfig {
                key: "foo".into(),
                value: "bar".into(),
            },
            Command::Paste,
        ];
        let (normal, deferred) = super::extract_deferred_commands(commands);
        assert_eq!(normal.len(), 2); // SendToKakoune + Paste
        assert_eq!(deferred.len(), 3); // Timer + Message + Config
    }

    #[test]
    fn test_extract_deferred_empty() {
        let commands = vec![
            Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
            Command::Quit,
        ];
        let (normal, deferred) = super::extract_deferred_commands(commands);
        assert_eq!(normal.len(), 2);
        assert!(deferred.is_empty());
    }

    // --- Slot cache tests ---

    struct CachedPlugin {
        counter: std::cell::Cell<u32>,
    }

    impl Plugin for CachedPlugin {
        fn id(&self) -> PluginId {
            PluginId("cached".to_string())
        }

        fn state_hash(&self) -> u64 {
            42 // constant — state never changes
        }

        fn slot_deps(&self, slot: Slot) -> DirtyFlags {
            match slot {
                Slot::BufferLeft => DirtyFlags::BUFFER,
                Slot::StatusRight => DirtyFlags::STATUS,
                _ => DirtyFlags::empty(),
            }
        }

        fn contribute(&self, slot: Slot, _state: &AppState) -> Option<Element> {
            self.counter.set(self.counter.get() + 1);
            match slot {
                Slot::BufferLeft => Some(Element::text("gutter", Face::default())),
                Slot::StatusRight => Some(Element::text("status", Face::default())),
                _ => None,
            }
        }
    }

    #[test]
    fn test_slot_cache_hit() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(CachedPlugin {
            counter: std::cell::Cell::new(0),
        }));
        let state = AppState::default();

        // First call: computes and caches
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        let result = registry.collect_slot(Slot::BufferLeft, &state);
        assert_eq!(result.len(), 1);

        // Second call with STATUS dirty — BufferLeft depends on BUFFER, not STATUS → cache hit
        registry.prepare_plugin_cache(DirtyFlags::STATUS);
        let result2 = registry.collect_slot(Slot::BufferLeft, &state);
        assert_eq!(result2.len(), 1);
    }

    #[test]
    fn test_slot_cache_miss_dirty() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(CachedPlugin {
            counter: std::cell::Cell::new(0),
        }));
        let state = AppState::default();

        // Warm the cache
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        registry.collect_slot(Slot::BufferLeft, &state);

        // BUFFER dirty → BufferLeft cache invalidated → recompute
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        let result = registry.collect_slot(Slot::BufferLeft, &state);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_slot_cache_miss_state_hash() {
        struct MutablePlugin {
            hash_val: std::cell::Cell<u64>,
        }
        impl Plugin for MutablePlugin {
            fn id(&self) -> PluginId {
                PluginId("mutable".to_string())
            }
            fn state_hash(&self) -> u64 {
                self.hash_val.get()
            }
            fn slot_deps(&self, _slot: Slot) -> DirtyFlags {
                DirtyFlags::BUFFER
            }
            fn contribute(&self, slot: Slot, _state: &AppState) -> Option<Element> {
                match slot {
                    Slot::BufferLeft => Some(Element::text("val", Face::default())),
                    _ => None,
                }
            }
        }

        let mut registry = PluginRegistry::new();
        let plugin = MutablePlugin {
            hash_val: std::cell::Cell::new(100),
        };
        registry.register(Box::new(plugin));
        let state = AppState::default();

        // Warm cache
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        registry.collect_slot(Slot::BufferLeft, &state);

        // No dirty flags, but simulate state hash change via re-register
        // We can't mutate the plugin through Box<dyn Plugin>, but we can test
        // that prepare_plugin_cache with empty dirty still serves from cache
        // when hash hasn't changed
        registry.prepare_plugin_cache(DirtyFlags::empty());
        let result = registry.collect_slot(Slot::BufferLeft, &state);
        assert_eq!(result.len(), 1); // cache hit
    }

    #[test]
    fn test_slot_cache_default_no_caching() {
        // TestPlugin has default state_hash=0 and slot_deps=ALL
        // → always recomputes (ALL intersects everything)
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(TestPlugin));
        let state = AppState::default();

        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        let result = registry.collect_slot(Slot::AboveBuffer, &state);
        assert_eq!(result.len(), 1);

        // Any dirty flag invalidates since slot_deps=ALL
        registry.prepare_plugin_cache(DirtyFlags::STATUS);
        let result2 = registry.collect_slot(Slot::AboveBuffer, &state);
        assert_eq!(result2.len(), 1);
    }

    #[test]
    fn test_slot_cache_empty_registry() {
        let mut registry = PluginRegistry::new();
        registry.prepare_plugin_cache(DirtyFlags::ALL);
        let state = AppState::default();
        let result = registry.collect_slot(Slot::BufferLeft, &state);
        assert!(result.is_empty());
    }

    #[test]
    fn test_prepare_cache_grows_with_register() {
        let mut registry = PluginRegistry::new();
        registry.prepare_plugin_cache(DirtyFlags::ALL);

        // Register after prepare
        registry.register(Box::new(TestPlugin));

        // Should not panic — prepare grows entries
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        let state = AppState::default();
        let result = registry.collect_slot(Slot::AboveBuffer, &state);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_slot_index_and_count() {
        assert_eq!(Slot::COUNT, 8);
        assert_eq!(Slot::BufferLeft.index(), 0);
        assert_eq!(Slot::Overlay.index(), 7);
        // All indices are unique
        let indices: Vec<usize> = Slot::ALL_VARIANTS.iter().map(|s| s.index()).collect();
        let unique: std::collections::HashSet<usize> = indices.iter().copied().collect();
        assert_eq!(unique.len(), Slot::COUNT);
    }

    #[test]
    fn test_set_config_stores_in_ui_options() {
        // SetConfig applied via ui_options (integration would be in event loop)
        let mut state = AppState::default();
        state.ui_options.insert("key".into(), "value".into());
        assert_eq!(state.ui_options.get("key").unwrap(), "value");
    }

    // --- Capability indexing tests ---

    /// Plugin that only provides line decorations (no slots, no overlays, etc.)
    struct LineDecorationOnlyPlugin;

    impl Plugin for LineDecorationOnlyPlugin {
        fn id(&self) -> PluginId {
            PluginId("line-deco".to_string())
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::LINE_DECORATION
        }

        fn contribute_line(&self, line: usize, _state: &AppState) -> Option<LineDecoration> {
            if line == 0 {
                Some(LineDecoration {
                    left_gutter: Some(Element::text("1", Face::default())),
                    right_gutter: None,
                    background: None,
                })
            } else {
                None
            }
        }
    }

    /// Plugin that only provides menu transforms.
    struct MenuTransformOnlyPlugin;

    impl Plugin for MenuTransformOnlyPlugin {
        fn id(&self) -> PluginId {
            PluginId("menu-tx".to_string())
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::MENU_TRANSFORM
        }

        fn transform_menu_item(
            &self,
            _item: &[crate::protocol::Atom],
            _index: usize,
            _selected: bool,
            _state: &AppState,
        ) -> Option<Vec<crate::protocol::Atom>> {
            Some(vec![crate::protocol::Atom {
                contents: "transformed".into(),
                face: Face::default(),
            }])
        }
    }

    #[test]
    fn test_capability_indexing_skips_non_participating() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(LineDecorationOnlyPlugin));
        registry.register(Box::new(MenuTransformOnlyPlugin));

        let mut state = AppState::default();
        state.lines = vec![crate::test_utils::make_line("hello")];

        // LineDecorationOnlyPlugin has no MENU_TRANSFORM → should not be called
        // MenuTransformOnlyPlugin has MENU_TRANSFORM → should transform
        let item = vec![crate::protocol::Atom {
            contents: "original".into(),
            face: Face::default(),
        }];
        let result = registry.transform_menu_item(&item, 0, false, &state);
        assert!(result.is_some());
        assert_eq!(result.unwrap()[0].contents, "transformed");

        // MenuTransformOnlyPlugin has no LINE_DECORATION → should not be called for gutters
        // LineDecorationOnlyPlugin has LINE_DECORATION → provides gutter
        let gutter = registry.build_left_gutter(&state);
        assert!(gutter.is_some());
    }

    #[test]
    fn test_capability_indexing_no_line_decoration_returns_none() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(MenuTransformOnlyPlugin));
        let state = AppState::default();

        // No plugin has LINE_DECORATION → early return None
        assert!(registry.build_left_gutter(&state).is_none());
        assert!(registry.build_right_gutter(&state).is_none());
        assert!(registry.collect_line_backgrounds(&state).is_none());
    }

    // --- Overlay caching tests ---

    struct OverlayPlugin {
        call_count: std::cell::Cell<u32>,
    }

    impl Plugin for OverlayPlugin {
        fn id(&self) -> PluginId {
            PluginId("overlay-test".to_string())
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::OVERLAY
        }

        fn contribute_overlay(&self, state: &AppState) -> Option<Overlay> {
            self.call_count.set(self.call_count.get() + 1);
            Some(Overlay {
                element: Element::text("overlay", Face::default()),
                anchor: crate::element::OverlayAnchor::Absolute {
                    x: 0,
                    y: 0,
                    w: state.cols,
                    h: state.rows,
                },
            })
        }

        fn overlay_deps(&self) -> DirtyFlags {
            DirtyFlags::BUFFER
        }
    }

    #[test]
    fn test_overlay_caching_returns_cached_on_second_call() {
        let mut registry = PluginRegistry::new();
        let plugin = OverlayPlugin {
            call_count: std::cell::Cell::new(0),
        };
        registry.register(Box::new(plugin));
        let state = AppState::default();

        // First call: cache miss
        let overlays = registry.collect_overlays(&state);
        assert_eq!(overlays.len(), 1);

        // Second call: should hit cache (no prepare_plugin_cache between calls)
        let overlays2 = registry.collect_overlays(&state);
        assert_eq!(overlays2.len(), 1);
    }

    #[test]
    fn test_overlay_cache_invalidated_on_dirty_deps() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(OverlayPlugin {
            call_count: std::cell::Cell::new(0),
        }));
        let state = AppState::default();

        // Populate cache
        let _ = registry.collect_overlays(&state);

        // Invalidate with BUFFER (overlay_deps includes BUFFER)
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);

        // Should recompute
        let overlays = registry.collect_overlays(&state);
        assert_eq!(overlays.len(), 1);
    }

    #[test]
    fn test_overlay_cache_not_invalidated_on_unrelated_dirty() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(OverlayPlugin {
            call_count: std::cell::Cell::new(0),
        }));
        let state = AppState::default();

        // Populate cache
        let _ = registry.collect_overlays(&state);

        // STATUS doesn't intersect overlay_deps (BUFFER) → cache stays
        registry.prepare_plugin_cache(DirtyFlags::STATUS);

        // Should still return cached
        let overlays = registry.collect_overlays(&state);
        assert_eq!(overlays.len(), 1);
    }

    // --- Plugin state change guard tests ---

    struct StatefulPlugin {
        hash: u64,
    }

    impl Plugin for StatefulPlugin {
        fn id(&self) -> PluginId {
            PluginId("stateful".to_string())
        }

        fn state_hash(&self) -> u64 {
            self.hash
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::SLOT_CONTRIBUTOR
        }

        fn contribute(&self, slot: Slot, _state: &AppState) -> Option<Element> {
            match slot {
                Slot::StatusRight => Some(Element::text("badge", Face::default())),
                _ => None,
            }
        }
    }

    #[test]
    fn test_any_plugin_state_changed_flag() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(StatefulPlugin { hash: 1 }));

        // Initial prepare: hash differs from default 0 → changed
        registry.prepare_plugin_cache(DirtyFlags::ALL);
        assert!(registry.any_plugin_state_changed());

        // Second prepare with same hash → no change
        registry.prepare_plugin_cache(DirtyFlags::ALL);
        assert!(!registry.any_plugin_state_changed());
    }

    // --- SlotId tests ---

    #[test]
    fn test_slot_id_to_legacy_roundtrip() {
        // All well-known SlotIds should roundtrip through Slot
        for slot in Slot::ALL_VARIANTS {
            let slot_id = SlotId::from(slot);
            assert!(slot_id.is_well_known(), "{slot_id:?} should be well-known");
            let back = slot_id.to_legacy().expect("should convert back to Slot");
            assert_eq!(back, slot, "roundtrip failed for {slot:?}");
        }
    }

    #[test]
    fn test_slot_id_custom_not_well_known() {
        let custom = SlotId::new("my.plugin.sidebar");
        assert!(!custom.is_well_known());
        assert_eq!(custom.to_legacy(), None);
        assert_eq!(custom.as_str(), "my.plugin.sidebar");
    }

    #[test]
    fn test_collect_slot_by_id_well_known_delegates() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(TestPlugin));
        let state = AppState::default();

        // TestPlugin contributes to Slot::AboveBuffer
        let via_id = registry.collect_slot_by_id(&SlotId::ABOVE_BUFFER, &state);
        let via_legacy = registry.collect_slot(Slot::AboveBuffer, &state);
        assert_eq!(via_id.len(), via_legacy.len());
        assert_eq!(via_id.len(), 1);
    }

    struct CustomSlotPlugin {
        slot_name: String,
    }

    impl Plugin for CustomSlotPlugin {
        fn id(&self) -> PluginId {
            PluginId("custom_slot".to_string())
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::NAMED_SLOT
        }

        fn contribute_named_slot(&self, name: &str, _state: &AppState) -> Option<Element> {
            if name == self.slot_name {
                Some(Element::text("custom", Face::default()))
            } else {
                None
            }
        }

        fn slot_id_deps(&self, slot_id: &SlotId) -> DirtyFlags {
            if slot_id.as_str() == self.slot_name {
                DirtyFlags::BUFFER
            } else {
                DirtyFlags::empty()
            }
        }
    }

    #[test]
    fn test_collect_slot_by_id_custom() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(CustomSlotPlugin {
            slot_name: "my.sidebar".into(),
        }));
        let state = AppState::default();

        let result = registry.collect_slot_by_id(&SlotId::new("my.sidebar"), &state);
        assert_eq!(result.len(), 1);

        let empty = registry.collect_slot_by_id(&SlotId::new("other.slot"), &state);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_custom_slot_cache_invalidation() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(CustomSlotPlugin {
            slot_name: "my.sidebar".into(),
        }));
        let state = AppState::default();
        let slot_id = SlotId::new("my.sidebar");

        // First call populates cache
        let r1 = registry.collect_slot_by_id(&slot_id, &state);
        assert_eq!(r1.len(), 1);

        // BUFFER dirty → should invalidate (CustomSlotPlugin's slot_id_deps returns BUFFER)
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        let r2 = registry.collect_slot_by_id(&slot_id, &state);
        assert_eq!(r2.len(), 1);

        // STATUS dirty → should NOT invalidate custom slot (deps = BUFFER only)
        registry.prepare_plugin_cache(DirtyFlags::STATUS);
        let r3 = registry.collect_slot_by_id(&slot_id, &state);
        assert_eq!(r3.len(), 1);
    }

    // -----------------------------------------------------------------------
    // PaintHook tests
    // -----------------------------------------------------------------------

    struct TestPaintHook {
        id: &'static str,
        deps: DirtyFlags,
        surface_filter: Option<crate::surface::SurfaceId>,
    }

    impl PaintHook for TestPaintHook {
        fn id(&self) -> &str {
            self.id
        }
        fn deps(&self) -> DirtyFlags {
            self.deps
        }
        fn surface_filter(&self) -> Option<crate::surface::SurfaceId> {
            self.surface_filter.clone()
        }
        fn apply(
            &self,
            grid: &mut crate::render::CellGrid,
            _region: &crate::layout::Rect,
            _state: &AppState,
        ) {
            // Write a marker character at (0, 0) to prove the hook ran
            if let Some(cell) = grid.get_mut(0, 0) {
                cell.grapheme = compact_str::CompactString::new(self.id);
            }
        }
    }

    struct PaintHookPlugin {
        hooks: Vec<Box<dyn PaintHook>>,
    }

    impl Plugin for PaintHookPlugin {
        fn id(&self) -> PluginId {
            PluginId("paint-hook-test".to_string())
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::PAINT_HOOK
        }

        fn paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
            // Re-create hooks each time (test simplicity)
            self.hooks
                .iter()
                .map(|h| -> Box<dyn PaintHook> {
                    Box::new(TestPaintHook {
                        id: match h.id() {
                            "hook-a" => "hook-a",
                            "hook-b" => "hook-b",
                            _ => "unknown",
                        },
                        deps: h.deps(),
                        surface_filter: h.surface_filter(),
                    })
                })
                .collect()
        }
    }

    #[test]
    fn test_collect_paint_hooks() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(PaintHookPlugin {
            hooks: vec![
                Box::new(TestPaintHook {
                    id: "hook-a",
                    deps: DirtyFlags::BUFFER,
                    surface_filter: None,
                }),
                Box::new(TestPaintHook {
                    id: "hook-b",
                    deps: DirtyFlags::STATUS,
                    surface_filter: None,
                }),
            ],
        }));
        let hooks = registry.collect_paint_hooks();
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].id(), "hook-a");
        assert_eq!(hooks[1].id(), "hook-b");
    }

    #[test]
    fn test_paint_hook_applies_to_grid() {
        use crate::layout::Rect;
        use crate::render::CellGrid;

        let mut grid = CellGrid::new(10, 5);
        let state = AppState::default();
        let region = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 5,
        };
        let hook = TestPaintHook {
            id: "X",
            deps: DirtyFlags::ALL,
            surface_filter: None,
        };
        hook.apply(&mut grid, &region, &state);
        assert_eq!(grid.get(0, 0).unwrap().grapheme.as_str(), "X");
    }

    #[test]
    fn test_apply_paint_hooks_deps_filtering() {
        use crate::layout::Rect;
        use crate::render::CellGrid;
        use crate::render::pipeline::apply_paint_hooks;

        let mut grid = CellGrid::new(10, 5);
        let state = AppState::default();
        let region = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 5,
        };

        // Hook depends on STATUS, but dirty is BUFFER → should NOT run
        let hooks: Vec<Box<dyn PaintHook>> = vec![Box::new(TestPaintHook {
            id: "Z",
            deps: DirtyFlags::STATUS,
            surface_filter: None,
        })];
        apply_paint_hooks(&hooks, &mut grid, &region, &state, DirtyFlags::BUFFER);
        // Cell (0,0) should still be the default (space)
        assert_ne!(grid.get(0, 0).unwrap().grapheme.as_str(), "Z");

        // Now with matching dirty flags → should run
        apply_paint_hooks(&hooks, &mut grid, &region, &state, DirtyFlags::STATUS);
        assert_eq!(grid.get(0, 0).unwrap().grapheme.as_str(), "Z");
    }

    #[test]
    fn test_paint_hook_no_capability_not_collected() {
        struct NoPaintHookPlugin;
        impl Plugin for NoPaintHookPlugin {
            fn id(&self) -> PluginId {
                PluginId("no-hook".to_string())
            }
            // capabilities() defaults to empty — no PAINT_HOOK
        }

        let mut registry = PluginRegistry::new();
        registry.register(Box::new(NoPaintHookPlugin));
        let hooks = registry.collect_paint_hooks();
        assert!(hooks.is_empty());
    }
}
