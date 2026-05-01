use proptest::prelude::*;

use super::*;
use crate::display::{BufferLine, InlineBoxAlignment, assert_display_map_invariants};
use crate::protocol::{Atom, Style, WireFace};
use crate::state::shadow_cursor::{EditProjection, EditableSpan};

fn pid(name: &str) -> PluginId {
    PluginId(name.to_string())
}

#[test]
fn shadow_cursor_inline_box_disjoint_lines_passes() {
    let mut set = DirectiveSet::default();
    let (ib, ib_owner) = inline_box(2, 5, "plugin_a");
    set.push(ib, 0, ib_owner);
    let (ed, ed_owner) = editable_with_span(0, 0, 0..10, "plugin_b");
    set.push(ed, 0, ed_owner);
    // Inline-box on line 2; editable span anchored on line 0. No collision.
    let _ = resolve(&set, 10);
}

#[test]
fn shadow_cursor_inline_box_anchor_outside_span_passes() {
    let mut set = DirectiveSet::default();
    // Inline-box at byte 20 on line 0; editable span covers 0..10 on line 0.
    // Same line but disjoint byte ranges → no collision.
    let (ib, ib_owner) = inline_box(0, 20, "plugin_a");
    set.push(ib, 0, ib_owner);
    let (ed, ed_owner) = editable_with_span(0, 0, 0..10, "plugin_b");
    set.push(ed, 0, ed_owner);
    let _ = resolve(&set, 10);
}

#[test]
#[should_panic(expected = "ShadowCursor × InlineBox overlap")]
fn shadow_cursor_inline_box_anchor_inside_span_panics_in_debug() {
    let mut set = DirectiveSet::default();
    // Inline-box anchor at byte 5; span covers 3..8 on the same line.
    let (ib, ib_owner) = inline_box(0, 5, "plugin_a");
    set.push(ib, 0, ib_owner);
    let (ed, ed_owner) = editable_with_span(0, 0, 3..8, "plugin_b");
    set.push(ed, 0, ed_owner);
    let _ = resolve(&set, 10);
}

#[test]
#[should_panic(expected = "ShadowCursor × InlineBox overlap")]
fn shadow_cursor_inline_box_anchor_at_span_start_panics_in_debug() {
    let mut set = DirectiveSet::default();
    // Boundary case: inline-box anchor coincides with span start.
    // The check is inclusive, so this must trigger.
    let (ib, ib_owner) = inline_box(0, 5, "plugin_a");
    set.push(ib, 0, ib_owner);
    let (ed, ed_owner) = editable_with_span(0, 0, 5..10, "plugin_b");
    set.push(ed, 0, ed_owner);
    let _ = resolve(&set, 10);
}

#[test]
#[should_panic(expected = "ShadowCursor × InlineBox overlap")]
fn shadow_cursor_inline_box_anchor_at_span_end_panics_in_debug() {
    let mut set = DirectiveSet::default();
    // Boundary case: inline-box anchor coincides with span end (inclusive
    // upper bound — the cursor would land on the same cell).
    let (ib, ib_owner) = inline_box(0, 10, "plugin_a");
    set.push(ib, 0, ib_owner);
    let (ed, ed_owner) = editable_with_span(0, 0, 5..10, "plugin_b");
    set.push(ed, 0, ed_owner);
    let _ = resolve(&set, 10);
}

fn inline_box(line: usize, byte_offset: usize, owner: &str) -> (DisplayDirective, PluginId) {
    (
        DisplayDirective::InlineBox {
            line,
            byte_offset,
            width_cells: 3.0,
            height_lines: 1.0,
            box_id: byte_offset as u64,
            alignment: InlineBoxAlignment::Center,
        },
        pid(owner),
    )
}

fn editable_with_span(
    after: usize,
    anchor_line: usize,
    buffer_range: std::ops::Range<usize>,
    owner: &str,
) -> (DisplayDirective, PluginId) {
    (
        DisplayDirective::EditableVirtualText {
            after,
            content: vec![],
            editable_spans: vec![EditableSpan {
                display_byte_range: 0..buffer_range.len(),
                anchor_line,
                buffer_byte_range: buffer_range,
                projection: EditProjection::Mirror,
            }],
        },
        pid(owner),
    )
}

#[test]
fn resolve_empty() {
    let set = DirectiveSet::default();
    assert_eq!(resolve(&set, 10), vec![]);
}

#[test]
fn resolve_single_plugin() {
    let mut set = DirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 1..3 }, 0, pid("a"));
    let result = resolve(&set, 10);
    assert_eq!(result, vec![DisplayDirective::Hide { range: 1..3 }]);
}

#[test]
fn resolve_hides_union() {
    let mut set = DirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 1..3 }, 0, pid("a"));
    set.push(DisplayDirective::Hide { range: 4..6 }, 0, pid("b"));
    let result = resolve(&set, 10);
    assert_eq!(result.len(), 2);
}

#[test]
fn resolve_hides_overlap_idempotent() {
    let mut set = DirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 1..4 }, 0, pid("a"));
    set.push(DisplayDirective::Hide { range: 2..5 }, 0, pid("b"));
    let result = resolve(&set, 10);
    // Both hides are emitted; DisplayMap::build handles the union via its hidden[] array
    assert_eq!(result.len(), 2);
    // Build the map to verify net effect: lines 1..5 hidden
    let dm = crate::display::DisplayMap::build(10, &result);
    assert_eq!(dm.buffer_to_display(BufferLine(1)), None);
    assert_eq!(dm.buffer_to_display(BufferLine(2)), None);
    assert_eq!(dm.buffer_to_display(BufferLine(3)), None);
    assert_eq!(dm.buffer_to_display(BufferLine(4)), None);
    assert!(dm.buffer_to_display(BufferLine(0)).is_some());
    assert!(dm.buffer_to_display(BufferLine(5)).is_some());
}

#[test]
fn resolve_folds_disjoint_both_kept() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 1..3,
            summary: vec![Atom::plain("fold-a")],
        },
        0,
        pid("a"),
    );
    set.push(
        DisplayDirective::Fold {
            range: 5..7,
            summary: vec![Atom::plain("fold-b")],
        },
        0,
        pid("b"),
    );
    let result = resolve(&set, 10);
    let fold_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::Fold { .. }))
        .count();
    assert_eq!(fold_count, 2);
}

#[test]
fn resolve_folds_overlap_higher_priority_wins() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 2..6,
            summary: vec![Atom::plain("low")],
        },
        0,
        pid("low"),
    );
    set.push(
        DisplayDirective::Fold {
            range: 3..8,
            summary: vec![Atom::plain("high")],
        },
        10,
        pid("high"),
    );
    let result = resolve(&set, 10);
    let folds: Vec<&str> = result
        .iter()
        .filter_map(|d| match d {
            DisplayDirective::Fold { summary, .. } => summary.first().map(|a| a.contents.as_str()),
            _ => None,
        })
        .collect();
    // Higher priority fold kept, lower dropped (overlap)
    assert_eq!(folds, vec!["high"]);
}

#[test]
fn resolve_folds_overlap_same_priority_plugin_id_tiebreak() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 1..5,
            summary: vec![Atom::plain("alpha")],
        },
        0,
        pid("alpha"),
    );
    set.push(
        DisplayDirective::Fold {
            range: 3..7,
            summary: vec![Atom::plain("beta")],
        },
        0,
        pid("beta"),
    );
    let result = resolve(&set, 10);
    let folds: Vec<&str> = result
        .iter()
        .filter_map(|d| match d {
            DisplayDirective::Fold { summary, .. } => summary.first().map(|a| a.contents.as_str()),
            _ => None,
        })
        .collect();
    // Same priority → plugin_id "alpha" < "beta" → "alpha" wins
    assert_eq!(folds, vec!["alpha"]);
}

#[test]
fn resolve_fold_hide_partial_overlap_fold_removed() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 2..6,
            summary: vec![Atom::plain("fold")],
        },
        0,
        pid("a"),
    );
    // Hide partially overlaps the fold range
    set.push(DisplayDirective::Hide { range: 4..8 }, 0, pid("b"));
    let result = resolve(&set, 10);
    // Fold should be removed (partial hide invalidates fold summary)
    let fold_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::Fold { .. }))
        .count();
    assert_eq!(fold_count, 0);
    // Hide still present
    assert!(
        result
            .iter()
            .any(|d| matches!(d, DisplayDirective::Hide { .. }))
    );
}

#[test]
fn resolve_fold_hide_full_cover_fold_removed() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 2..5,
            summary: vec![Atom::plain("fold")],
        },
        0,
        pid("a"),
    );
    // Hide fully covers the fold range
    set.push(DisplayDirective::Hide { range: 1..6 }, 0, pid("b"));
    let result = resolve(&set, 10);
    let fold_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::Fold { .. }))
        .count();
    assert_eq!(fold_count, 0);
}

#[test]
fn resolve_fold_hide_disjoint_both_kept() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 1..3,
            summary: vec![Atom::plain("fold")],
        },
        0,
        pid("a"),
    );
    set.push(DisplayDirective::Hide { range: 5..7 }, 0, pid("b"));
    let result = resolve(&set, 10);
    let fold_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::Fold { .. }))
        .count();
    let hide_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::Hide { .. }))
        .count();
    assert_eq!(fold_count, 1);
    assert_eq!(hide_count, 1);
}

// --- resolve_inline tests ---

#[test]
fn resolve_inline_empty() {
    let result = super::resolve_inline(&[]);
    assert!(result.is_empty());
}

#[test]
fn resolve_inline_single_style() {
    use crate::protocol::Color;
    let face = WireFace {
        fg: Color::Named(crate::protocol::NamedColor::Red),
        ..WireFace::default()
    };
    let td = TaggedDirective {
        directive: DisplayDirective::StyleInline {
            line: 5,
            byte_range: 3..8,
            face,
        },
        priority: 0,
        plugin_id: pid("a"),
    };
    let result = super::resolve_inline(&[td]);
    assert_eq!(result.len(), 1);
    let deco = &result[&5];
    assert_eq!(deco.ops().len(), 1);
    match &deco.ops()[0] {
        crate::render::inline_decoration::InlineOp::Style { range, face: f } => {
            assert_eq!(range, &(3..8));
            assert_eq!(f.fg, Color::Named(crate::protocol::NamedColor::Red));
        }
        _ => panic!("expected Style op"),
    }
}

#[test]
fn resolve_inline_overlapping_styles_split() {
    use crate::protocol::Color;
    let red_face = WireFace {
        fg: Color::Named(crate::protocol::NamedColor::Red),
        ..WireFace::default()
    };
    let blue_face = WireFace {
        bg: Color::Named(crate::protocol::NamedColor::Blue),
        ..WireFace::default()
    };
    // Plugin "a" styles 2..8, plugin "b" styles 5..12. Overlap at 5..8.
    let tds = vec![
        TaggedDirective {
            directive: DisplayDirective::StyleInline {
                line: 0,
                byte_range: 2..8,
                face: red_face,
            },
            priority: 0,
            plugin_id: pid("a"),
        },
        TaggedDirective {
            directive: DisplayDirective::StyleInline {
                line: 0,
                byte_range: 5..12,
                face: blue_face,
            },
            priority: 10,
            plugin_id: pid("b"),
        },
    ];
    let result = super::resolve_inline(&tds);
    let deco = &result[&0];
    // Should have 3 segments: [2..5] red only, [5..8] red+blue merged, [8..12] blue only
    let ops = deco.ops();
    assert_eq!(ops.len(), 3, "expected 3 style segments, got {:?}", ops);

    // Verify non-overlapping ranges (INV-INLINE-2)
    let mut prev_end = 0;
    for op in ops {
        if let crate::render::inline_decoration::InlineOp::Style { range, .. } = op {
            assert!(
                range.start >= prev_end,
                "overlapping range: prev_end={prev_end}, start={}",
                range.start
            );
            prev_end = range.end;
        }
    }
}

#[test]
fn resolve_inline_hide_suppresses_style() {
    use crate::protocol::Color;
    let face = WireFace {
        fg: Color::Named(crate::protocol::NamedColor::Red),
        ..WireFace::default()
    };
    // Style 2..10, Hide 4..7 — style should be split around the hidden region
    let tds = vec![
        TaggedDirective {
            directive: DisplayDirective::StyleInline {
                line: 0,
                byte_range: 2..10,
                face,
            },
            priority: 0,
            plugin_id: pid("a"),
        },
        TaggedDirective {
            directive: DisplayDirective::HideInline {
                line: 0,
                byte_range: 4..7,
            },
            priority: 0,
            plugin_id: pid("b"),
        },
    ];
    let result = super::resolve_inline(&tds);
    let deco = &result[&0];
    let ops = deco.ops();

    // Should have: Style{2..4}, Hide{4..7}, Style{7..10}
    let style_count = ops
        .iter()
        .filter(|o| matches!(o, crate::render::inline_decoration::InlineOp::Style { .. }))
        .count();
    let hide_count = ops
        .iter()
        .filter(|o| matches!(o, crate::render::inline_decoration::InlineOp::Hide { .. }))
        .count();
    assert_eq!(style_count, 2);
    assert_eq!(hide_count, 1);
}

#[test]
fn resolve_inline_insert_ordering() {
    use crate::protocol::Color;
    let red_face = WireFace {
        fg: Color::Named(crate::protocol::NamedColor::Red),
        ..WireFace::default()
    };
    let tds = vec![
        TaggedDirective {
            directive: DisplayDirective::InsertInline {
                line: 0,
                byte_offset: 5,
                content: vec![Atom::with_style("X", Style::from_face(&red_face))],
                interaction: crate::display::InlineInteraction::None,
            },
            priority: 0,
            plugin_id: pid("a"),
        },
        TaggedDirective {
            directive: DisplayDirective::InsertInline {
                line: 0,
                byte_offset: 5,
                content: vec![Atom::plain("Y")],
                interaction: crate::display::InlineInteraction::None,
            },
            priority: 10,
            plugin_id: pid("b"),
        },
    ];
    let result = super::resolve_inline(&tds);
    let deco = &result[&0];
    let ops = deco.ops();
    assert_eq!(ops.len(), 2);
    // Higher priority first (priority desc sort)
    match &ops[0] {
        crate::render::inline_decoration::InlineOp::Insert { content, .. } => {
            assert_eq!(content[0].contents.as_str(), "Y");
        }
        _ => panic!("expected Insert"),
    }
}

#[test]
fn resolve_inline_multi_line() {
    let face = WireFace::default();
    let tds = vec![
        TaggedDirective {
            directive: DisplayDirective::StyleInline {
                line: 0,
                byte_range: 0..5,
                face,
            },
            priority: 0,
            plugin_id: pid("a"),
        },
        TaggedDirective {
            directive: DisplayDirective::StyleInline {
                line: 3,
                byte_range: 2..7,
                face,
            },
            priority: 0,
            plugin_id: pid("a"),
        },
    ];
    let result = super::resolve_inline(&tds);
    assert_eq!(result.len(), 2);
    assert!(result.contains_key(&0));
    assert!(result.contains_key(&3));
}

// --- Phase 5: proptest for resolve → build pipeline ---

fn arb_display_directive(max_line: usize) -> impl Strategy<Value = DisplayDirective> {
    let m = max_line.max(1);
    prop_oneof![
        (0usize..m, 1usize..m.min(8).max(1) + 1).prop_map(move |(s, len)| {
            DisplayDirective::Fold {
                range: s..(s + len).min(m),
                summary: vec![Atom::plain("...")],
            }
        }),
        (0usize..m, 1usize..m.min(8).max(1) + 1).prop_map(move |(s, len)| {
            DisplayDirective::Hide {
                range: s..(s + len).min(m),
            }
        }),
    ]
}

fn arb_tagged_directives(max_line: usize) -> impl Strategy<Value = DirectiveSet> {
    let m = max_line.max(1);
    prop::collection::vec(
        (
            arb_display_directive(m),
            -10i16..10i16,
            "[a-z]{1,4}".prop_map(PluginId),
        ),
        0..8,
    )
    .prop_map(|items| {
        let mut set = DirectiveSet::default();
        for (d, priority, plugin_id) in items {
            set.push(d, priority, plugin_id);
        }
        set
    })
}

fn ranges_overlap(a: &std::ops::Range<usize>, b: &std::ops::Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn resolve_build_invariants(
        (line_count, set) in (1usize..50).prop_flat_map(|lc| {
            (Just(lc), arb_tagged_directives(lc))
        })
    ) {
        let resolved = resolve(&set, line_count);
        let dm = crate::display::DisplayMap::build(line_count, &resolved);
        assert_display_map_invariants(&dm, line_count);
    }

    #[test]
    fn resolve_no_fold_hide_overlap(
        (line_count, set) in (1usize..50).prop_flat_map(|lc| {
            (Just(lc), arb_tagged_directives(lc))
        })
    ) {
        let resolved = resolve(&set, line_count);
        for d1 in &resolved {
            if let DisplayDirective::Fold { range: fold_r, .. } = d1 {
                for d2 in &resolved {
                    if let DisplayDirective::Hide { range: hide_r } = d2 {
                        prop_assert!(
                            !ranges_overlap(fold_r, hide_r),
                            "resolve produced fold {fold_r:?} overlapping hide {hide_r:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn shadow_cursor_inline_box_no_collision_when_lines_differ(
        line_a in 0usize..20,
        line_b in 0usize..20,
        offset in 0usize..32,
        span_start in 0usize..32,
        span_len in 1usize..16,
    ) {
        prop_assume!(line_a != line_b);
        let mut set = DirectiveSet::default();
        let (ib, ib_owner) = inline_box(line_a, offset, "plugin_a");
        set.push(ib, 0, ib_owner);
        let (ed, ed_owner) = editable_with_span(
            line_b,
            line_b,
            span_start..span_start + span_len,
            "plugin_b",
        );
        set.push(ed, 0, ed_owner);
        // Different anchor lines: must never trigger the assertion.
        let _ = resolve(&set, 30);
    }

    #[test]
    fn resolve_no_overlapping_folds(
        (line_count, set) in (1usize..50).prop_flat_map(|lc| {
            (Just(lc), arb_tagged_directives(lc))
        })
    ) {
        let resolved = resolve(&set, line_count);
        let folds: Vec<_> = resolved.iter().filter_map(|d| {
            if let DisplayDirective::Fold { range, .. } = d {
                Some(range.clone())
            } else {
                None
            }
        }).collect();
        for i in 0..folds.len() {
            for j in i + 1..folds.len() {
                prop_assert!(
                    !ranges_overlap(&folds[i], &folds[j]),
                    "resolve produced overlapping folds {:?} and {:?}",
                    folds[i], folds[j]
                );
            }
        }
    }
}
