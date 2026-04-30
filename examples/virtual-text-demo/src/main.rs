//! Content annotation proof artifact — demonstrates ContentAnnotation-based inline annotations.
//!
//! This native Plugin proves that:
//! 1. ContentAnnotation inserts rich Element content between buffer lines
//! 2. Mouse clicks on annotation content are suppressed via SegmentMap
//! 3. Buffer editing works correctly with annotations present
//!
//! Usage:
//!   cargo run --manifest-path examples/virtual-text-demo/Cargo.toml -- [file]
//!
//! Press `z` to toggle content annotations on/off.
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
        icon: "\u{26d4}", // ⛔
        label: "known defect \u{2014} fix required",
        color: NamedColor::Red,
    },
    Keyword {
        pattern: "TODO",
        icon: "\u{26a0}", // ⚠
        label: "consider addressing before merge",
        color: NamedColor::Yellow,
    },
    Keyword {
        pattern: "HACK",
        icon: "\u{26a1}", // ⚡
        label: "temporary workaround \u{2014} needs proper solution",
        color: NamedColor::Magenta,
    },
    Keyword {
        pattern: "NOTE",
        icon: "\u{2139}", // ℹ
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
struct AnnotationDemoState {
    enabled: bool,
}

struct ContentAnnotationDemoPlugin;

impl Plugin for ContentAnnotationDemoPlugin {
    type State = AnnotationDemoState;

    fn id(&self) -> PluginId {
        PluginId("virtual_text_demo".into())
    }

    fn register(&self, r: &mut HandlerRegistry<AnnotationDemoState>) {
        r.on_key(|state, key, _app| {
            if key.key == Key::Char('z') && key.modifiers.is_empty() {
                Some((
                    AnnotationDemoState {
                        enabled: !state.enabled,
                    },
                    vec![Command::RequestRedraw(DirtyFlags::ALL)],
                ))
            } else {
                None
            }
        });

        r.on_content_annotation(|state, app, _ctx| {
            if !state.enabled {
                return vec![];
            }

            let mut annotations = Vec::new();
            for (i, line) in app.lines().iter().enumerate() {
                let text = line_text(line);
                if let Some(kw) = detect_keyword(&text) {
                    let label = format!("  {} {} \u{2014} {}", kw.icon, kw.pattern, kw.label);
                    annotations.push(ContentAnnotation {
                        anchor: ContentAnchor::InsertAfter(i),
                        element: Element::text_with_style(
                            &label,
                            Style {
                                fg: Brush::Named(kw.color),
                                ..Style::default()
                            },
                        ),
                        plugin_id: PluginId("virtual_text_demo".into()),
                        priority: 0,
                    });
                }
            }
            annotations
        });

        r.on_contribute(SlotId::STATUS_RIGHT, |state, _app, _ctx| {
            let label = if state.enabled {
                " [annotations ON] "
            } else {
                " [annotations OFF] "
            };

            Some(Contribution {
                element: Element::text_with_style(
                    label,
                    Style {
                        fg: if state.enabled {
                            Brush::Named(NamedColor::Green)
                        } else {
                            Brush::Default
                        },
                        ..Style::default()
                    },
                ),
                priority: 0,
                size_hint: ContribSizeHint::Auto,
            })
        });
    }
}

fn main() {
    kasane::run_with_factories([host_plugin("virtual_text_demo", || {
        PluginBridge::new(ContentAnnotationDemoPlugin)
    })]);
}
