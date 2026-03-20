//! Virtual text proof artifact — demonstrates DisplayMap-based inline annotations.
//!
//! This native Plugin proves that:
//! 1. InsertAfter directives add virtual text lines between buffer lines
//! 2. Cursor movement naturally skips virtual text (SourceMapping::None)
//! 3. Mouse clicks on virtual text are suppressed (InteractionPolicy::ReadOnly)
//! 4. Buffer editing works correctly with virtual text present
//! 5. display_line_count > buffer_line_count when virtual text is active
//!
//! Usage:
//!   cargo run --manifest-path examples/virtual-text-demo/Cargo.toml -- [file]
//!
//! Press `z` to toggle virtual text annotations on/off.
//! Detects TODO, FIXME, HACK, NOTE keywords in buffer lines.

use kasane::kasane_core::plugin_prelude::*;

struct Keyword {
    pattern: &'static str,
    icon: &'static str,
    label: &'static str,
    color: NamedColor,
}

const KEYWORDS: &[Keyword] = &[
    Keyword {
        pattern: "FIXME",
        icon: "\u{26d4}",  // ⛔
        label: "known defect \u{2014} fix required",
        color: NamedColor::Red,
    },
    Keyword {
        pattern: "TODO",
        icon: "\u{26a0}",  // ⚠
        label: "consider addressing before merge",
        color: NamedColor::Yellow,
    },
    Keyword {
        pattern: "HACK",
        icon: "\u{26a1}",  // ⚡
        label: "temporary workaround \u{2014} needs proper solution",
        color: NamedColor::Magenta,
    },
    Keyword {
        pattern: "NOTE",
        icon: "\u{2139}",  // ℹ
        label: "important context for reviewers",
        color: NamedColor::Cyan,
    },
];

fn line_text(line: &[Atom]) -> String {
    line.iter().map(|a| a.contents.as_str()).collect()
}

fn detect_keyword(text: &str) -> Option<&'static Keyword> {
    KEYWORDS.iter().find(|kw| text.contains(kw.pattern))
}

#[derive(Clone, Debug, Default, PartialEq)]
struct VirtualTextState {
    enabled: bool,
}

struct VirtualTextDemoPlugin;

impl Plugin for VirtualTextDemoPlugin {
    type State = VirtualTextState;

    fn id(&self) -> PluginId {
        PluginId("virtual_text_demo".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::DISPLAY_TRANSFORM | PluginCapabilities::CONTRIBUTOR
    }

    fn handle_key(
        &self,
        state: &VirtualTextState,
        key: &KeyEvent,
        _app: &AppState,
    ) -> Option<(VirtualTextState, Vec<Command>)> {
        if key.key == Key::Char('z') && key.modifiers.is_empty() {
            Some((
                VirtualTextState {
                    enabled: !state.enabled,
                },
                vec![Command::RequestRedraw(DirtyFlags::ALL)],
            ))
        } else {
            None
        }
    }

    fn display_directives(
        &self,
        state: &VirtualTextState,
        app: &AppState,
    ) -> Vec<DisplayDirective> {
        if !state.enabled {
            return vec![];
        }

        let mut directives = Vec::new();
        for (i, line) in app.lines.iter().enumerate() {
            let text = line_text(line);
            if let Some(kw) = detect_keyword(&text) {
                directives.push(DisplayDirective::InsertAfter {
                    after: i,
                    content: format!("  {} {} \u{2014} {}", kw.icon, kw.pattern, kw.label),
                    face: Face {
                        fg: Color::Named(kw.color),
                        ..Face::default()
                    },
                });
            }
        }
        directives
    }

    fn contribute_to(
        &self,
        state: &VirtualTextState,
        region: &SlotId,
        _app: &AppState,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        if region != &SlotId::STATUS_RIGHT {
            return None;
        }

        let label = if state.enabled {
            " [annotations ON] "
        } else {
            " [annotations OFF] "
        };

        Some(Contribution {
            element: Element::text(
                label,
                Face {
                    fg: if state.enabled {
                        Color::Named(NamedColor::Green)
                    } else {
                        Color::Default
                    },
                    ..Face::default()
                },
            ),
            priority: 0,
            size_hint: ContribSizeHint::Auto,
        })
    }

}

fn main() {
    kasane::run(|registry| {
        registry.register(VirtualTextDemoPlugin);
    });
}
