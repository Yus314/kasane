use std::any::Any;
use std::io::Write;

use crate::element::{Element, InteractiveId, Overlay, OverlayAnchor};
use crate::input::{KeyEvent, MouseEvent};
use crate::layout::HitMap;
use crate::protocol::KasaneRequest;
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

pub enum Command {
    SendToKakoune(KasaneRequest),
    Paste,
    Quit,
    RequestRedraw(DirtyFlags),
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
}

pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
    hit_map: HitMap,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            plugins: Vec::new(),
            hit_map: HitMap::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    pub fn plugins_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn Plugin>> {
        self.plugins.iter_mut()
    }

    pub fn collect_slot(&self, slot: Slot, state: &AppState) -> Vec<Element> {
        self.plugins
            .iter()
            .filter_map(|p| p.contribute(slot, state))
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
}
