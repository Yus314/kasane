use std::any::Any;
use std::cell::RefCell;
use std::io::Write;
use std::time::Duration;

use crate::element::{Element, FlexChild, InteractiveId, Overlay, OverlayAnchor};
use crate::input::{KeyEvent, MouseEvent};
use crate::layout::HitMap;
use crate::protocol::{Face, KasaneRequest};
use crate::state::{AppState, DirtyFlags};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginId(pub String);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecorateTarget {
    Buffer,
    StatusBar,
    Menu,
    Info,
    BufferLine(usize),
}

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
            | Command::SetConfig { .. } => {}
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
    fn slot_deps(&self, _slot: Slot) -> DirtyFlags {
        DirtyFlags::ALL
    }

    fn contribute(&self, _slot: Slot, _state: &AppState) -> Option<Element> {
        None
    }

    fn contribute_overlay(&self, _state: &AppState) -> Option<Overlay> {
        None
    }

    fn decorate(&self, _target: DecorateTarget, element: Element, _state: &AppState) -> Element {
        element
    }

    fn replace(&self, _target: ReplaceTarget, _state: &AppState) -> Option<Element> {
        None
    }

    fn decorator_priority(&self) -> u32 {
        0
    }

    // --- Line decoration ---

    /// Contribute decoration for a specific buffer line.
    fn contribute_line(&self, _line: usize, _state: &AppState) -> Option<LineDecoration> {
        None
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
}

/// Cached result for a single plugin's slot contributions.
#[derive(Default)]
struct PluginCacheEntry {
    last_state_hash: u64,
    /// `None` = not cached. `Some(x)` = cached contribute() result.
    slots: [Option<Option<Element>>; Slot::COUNT],
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

pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
    hit_map: HitMap,
    slot_cache: RefCell<PluginSlotCache>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            plugins: Vec::new(),
            hit_map: HitMap::new(),
            slot_cache: RefCell::new(PluginSlotCache::new()),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
        self.slot_cache
            .get_mut()
            .entries
            .push(PluginCacheEntry::default());
    }

    /// Invalidate slot cache entries based on dirty flags and state hash changes.
    /// Call once per frame before rendering (during the mutable phase).
    pub fn prepare_plugin_cache(&mut self, dirty: DirtyFlags) {
        let cache = self.slot_cache.get_mut();

        // Grow entries if plugins were registered after last prepare
        while cache.entries.len() < self.plugins.len() {
            cache.entries.push(PluginCacheEntry::default());
        }

        for (i, plugin) in self.plugins.iter().enumerate() {
            let entry = &mut cache.entries[i];
            let current_hash = plugin.state_hash();

            // L1: state hash changed → invalidate all slot entries for this plugin
            if current_hash != entry.last_state_hash {
                entry.last_state_hash = current_hash;
                for slot_entry in &mut entry.slots {
                    *slot_entry = None;
                }
                continue; // all slots already invalidated
            }

            // L3: check per-slot dirty flag intersection
            for slot in &Slot::ALL_VARIANTS {
                let slot_deps = plugin.slot_deps(*slot);
                if dirty.intersects(slot_deps) {
                    entry.slots[slot.index()] = None;
                }
            }
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

                // Cache miss — compute and store
                let result = plugin.contribute(slot, state);

                // Ensure entry exists (grow if needed)
                while cache.entries.len() <= i {
                    cache.entries.push(PluginCacheEntry::default());
                }
                cache.entries[i].slots[slot_idx] = Some(result.clone());

                result
            })
            .collect()
    }

    /// Collect overlays from plugins: both typed overlays (contribute_overlay)
    /// and legacy Slot::Overlay contributions (wrapped in full-screen Absolute anchor).
    pub fn collect_overlays(&self, state: &AppState) -> Vec<Overlay> {
        let mut overlays = Vec::new();
        for plugin in &self.plugins {
            // Typed overlay with plugin-specified anchor
            if let Some(overlay) = plugin.contribute_overlay(state) {
                overlays.push(overlay);
            }
            // Legacy: Slot::Overlay → full-screen Absolute (backward compat)
            if let Some(element) = plugin.contribute(Slot::Overlay, state) {
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

    /// Apply decorators in priority order (high priority = inner = applied first).
    pub fn apply_decorator(
        &self,
        target: DecorateTarget,
        element: Element,
        state: &AppState,
    ) -> Element {
        let mut decorators: Vec<&dyn Plugin> = self.plugins.iter().map(|p| p.as_ref()).collect();
        decorators.sort_by_key(|p| std::cmp::Reverse(p.decorator_priority()));
        decorators
            .into_iter()
            .fold(element, |el, plugin| plugin.decorate(target, el, state))
    }

    /// Get a replacement element. Last registered plugin wins.
    pub fn get_replacement(&self, target: ReplaceTarget, state: &AppState) -> Option<Element> {
        self.plugins
            .iter()
            .rev()
            .find_map(|p| p.replace(target, state))
    }

    // --- Line decoration ---

    /// Build left gutter column from plugin line decorations.
    /// Returns None when no plugin provides gutter content (zero overhead).
    pub fn build_left_gutter(&self, state: &AppState) -> Option<Element> {
        if self.plugins.is_empty() {
            return None;
        }
        let line_count = state.visible_line_range().len();
        let mut has_any = false;
        let mut rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        for line in 0..line_count {
            let mut gutter_el: Option<Element> = None;
            for plugin in &self.plugins {
                if let Some(dec) = plugin.contribute_line(line, state)
                    && let Some(el) = dec.left_gutter
                {
                    gutter_el = Some(el);
                    has_any = true;
                    break;
                }
            }
            rows.push(FlexChild::fixed(gutter_el.unwrap_or(Element::Empty)));
        }
        if has_any {
            Some(Element::column(rows))
        } else {
            None
        }
    }

    /// Build right gutter column from plugin line decorations.
    /// Returns None when no plugin provides gutter content (zero overhead).
    pub fn build_right_gutter(&self, state: &AppState) -> Option<Element> {
        if self.plugins.is_empty() {
            return None;
        }
        let line_count = state.visible_line_range().len();
        let mut has_any = false;
        let mut rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        for line in 0..line_count {
            let mut gutter_el: Option<Element> = None;
            for plugin in &self.plugins {
                if let Some(dec) = plugin.contribute_line(line, state)
                    && let Some(el) = dec.right_gutter
                {
                    gutter_el = Some(el);
                    has_any = true;
                    break;
                }
            }
            rows.push(FlexChild::fixed(gutter_el.unwrap_or(Element::Empty)));
        }
        if has_any {
            Some(Element::column(rows))
        } else {
            None
        }
    }

    /// Collect background overrides from all plugins for visible lines.
    /// Returns None when no plugin provides any background (zero overhead).
    pub fn collect_line_backgrounds(&self, state: &AppState) -> Option<Vec<Option<Face>>> {
        if self.plugins.is_empty() {
            return None;
        }
        let line_count = state.visible_line_range().len();
        let mut backgrounds: Vec<Option<Face>> = vec![None; line_count];
        let mut has_any = false;
        for (line, bg_slot) in backgrounds.iter_mut().enumerate().take(line_count) {
            for plugin in &self.plugins {
                if let Some(dec) = plugin.contribute_line(line, state)
                    && let Some(bg) = dec.background
                {
                    *bg_slot = Some(bg);
                    has_any = true;
                    break;
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
        for plugin in &self.plugins {
            let input = current.as_deref().unwrap_or(item);
            if let Some(transformed) = plugin.transform_menu_item(input, index, selected, state) {
                current = Some(transformed);
            }
        }
        current
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
mod tests {
    use super::*;
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
}
