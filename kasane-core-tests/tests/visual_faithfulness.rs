//! Visual Faithfulness witness (§10.2a) — ADR-030 Level 4.
//!
//! From `docs/semantics.md` §10.2a:
//!
//! > Preserving transformations are visually faithful iff the spatial
//! > restructuring is reversible; Fold with a summary line that responds
//! > to an unfold command satisfies this because the fold toggle is a
//! > single interaction.

use kasane_core::display::{BufferLine, DisplayDirective, DisplayMap, FoldToggleState};
use kasane_core::plugin::{AppView, HandlerRegistry, Plugin, PluginId, PluginRuntime};
use kasane_core::protocol::Atom;
use kasane_core::state::AppState;

#[test]
fn fold_toggle_recovers_all_folded_lines() {
    let line_count = 30;
    let fold_range = 10..20;
    let directives = vec![DisplayDirective::Fold {
        range: fold_range.clone(),
        summary: vec![Atom::plain("…")],
    }];

    // Before toggle: folded lines map to a single summary display line.
    let dm_before = DisplayMap::build(line_count, &directives);
    // Lines 10..20 share a single display line (the fold summary).
    let summary_dl = dm_before.buffer_to_display(BufferLine(fold_range.start));
    assert!(summary_dl.is_some(), "fold summary must exist");
    for bl in fold_range.clone() {
        assert_eq!(
            dm_before.buffer_to_display(BufferLine(bl)),
            summary_dl,
            "all folded lines must map to the same summary"
        );
    }

    // Toggle: expand the fold.
    let mut toggle = FoldToggleState::default();
    toggle.toggle(&fold_range);
    let mut filtered = directives;
    toggle.filter_directives(&mut filtered);

    // After toggle: every previously-folded line has its own display line.
    let dm_after = DisplayMap::build(line_count, &filtered);
    for bl in fold_range {
        assert!(
            dm_after.buffer_to_display(BufferLine(bl)).is_some(),
            "after toggle, buffer line {bl} must be individually visible"
        );
    }
}

#[test]
fn fold_toggle_is_single_interaction() {
    let fold_range = 5..15;
    let line_count = 20;
    let directives = vec![DisplayDirective::Fold {
        range: fold_range.clone(),
        summary: vec![Atom::plain("…")],
    }];

    // A single toggle call is sufficient to expand the fold.
    let mut toggle = FoldToggleState::default();
    toggle.toggle(&fold_range);
    assert!(
        toggle.is_expanded(&fold_range),
        "one toggle must expand the fold"
    );

    let mut filtered = directives;
    toggle.filter_directives(&mut filtered);
    assert!(
        filtered.is_empty(),
        "expanded fold must be filtered out in one step"
    );

    // Verify all lines are now visible.
    let dm = DisplayMap::build(line_count, &filtered);
    for bl in fold_range {
        assert!(
            dm.buffer_to_display(BufferLine(bl)).is_some(),
            "line {bl} must be visible after single toggle"
        );
    }
}

// ---------------------------------------------------------------------------
// Premise audit for #107 — what actually filters destructive directives?
//
// #107's central claim is that `hide_inline()` from an `Unwitnessed`
// (`on_display`-registered) plugin is "silently dropped" because the
// recovery audit driven by ADR-030 §Level 4 rejects directives from
// non-faithful sources. The three tests below pin down what production
// code actually does:
//
//   1. `Unwitnessed` HideInline survives `collect_display_directives`.
//   2. `Unwitnessed` full-line Hide survives `collect_display_directives`.
//   3. A separate cursor-safety-net (not the recovery audit) does drop
//      full-line Hide when the cursor sits on the hidden line — but it
//      operates on the resolved directive shape, ignoring source plugin
//      identity, and does not touch HideInline at all.
//
// Together these witness that the recovery-witness filter described in
// #107 does not exist at runtime today. ADR-030 §Level 4 shipped the
// classification infrastructure (`DisplayRecoveryStatus`,
// `is_visually_faithful`) but no production consumer reads it. The
// markdown-rich workaround motivated by the "silently dropped" claim is
// addressing a symptom that has another cause (or no cause at all).
// ---------------------------------------------------------------------------

struct UnwitnessedHideInlinePlugin;

impl Plugin for UnwitnessedHideInlinePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId::from("unwitnessed-hide-inline-probe")
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.on_display(|_state, _app| {
            vec![DisplayDirective::HideInline {
                line: 0,
                byte_range: 0..3,
            }]
        });
    }
}

struct UnwitnessedHidePlugin;

impl Plugin for UnwitnessedHidePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId::from("unwitnessed-hide-probe")
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.on_display(|_state, _app| vec![DisplayDirective::Hide { range: 1..3 }]);
    }
}

#[test]
fn unwitnessed_hide_inline_reaches_resolved_output() {
    let mut registry = PluginRuntime::new();
    registry.register(UnwitnessedHideInlinePlugin);

    let mut state = AppState::default();
    state.observed.lines = vec![vec![Atom::plain("hello world")], vec![], vec![]].into();

    let directives = registry
        .view()
        .collect_display_directives(&AppView::new(&state));

    assert!(
        directives.iter().any(|d| matches!(
            d,
            DisplayDirective::HideInline { line: 0, byte_range }
                if byte_range.start == 0 && byte_range.end == 3
        )),
        "Unwitnessed HideInline must reach resolved output \
         (recovery audit is not wired up); got {:?}",
        directives,
    );
}

#[test]
fn unwitnessed_full_hide_reaches_resolved_output() {
    let mut registry = PluginRuntime::new();
    registry.register(UnwitnessedHidePlugin);

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![], vec![]].into();

    let directives = registry
        .view()
        .collect_display_directives(&AppView::new(&state));

    assert!(
        directives.iter().any(
            |d| matches!(d, DisplayDirective::Hide { range } if range.start == 1 && range.end == 3)
        ),
        "Unwitnessed Hide must reach `collect_display_directives` output \
         (no recovery audit); got {:?}",
        directives,
    );
}

#[test]
fn cursor_safety_net_drops_full_hide_at_cursor_line_only() {
    // The cursor safety net in `collect_display_map` removes any `Hide`
    // whose range contains the cursor line, regardless of source plugin's
    // recovery status. It is the only production filter that currently
    // touches destructive directives, and it does not touch HideInline.

    let mut registry = PluginRuntime::new();
    registry.register(UnwitnessedHidePlugin); // emits Hide { range: 1..3 }

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![], vec![]].into();

    // Cursor on line 0 (outside the Hide range): directive survives.
    state.observed.cursor_pos.line = 0;
    let dm_outside = registry.view().collect_display_map(&AppView::new(&state));
    assert!(
        dm_outside.buffer_to_display(BufferLine(1)).is_none(),
        "Hide range 1..3 must take effect when cursor is on line 0"
    );

    // Cursor on line 2 (inside the Hide range): cursor safety net drops it.
    state.observed.cursor_pos.line = 2;
    let dm_inside = registry.view().collect_display_map(&AppView::new(&state));
    assert!(
        dm_inside.buffer_to_display(BufferLine(2)).is_some(),
        "Cursor safety net must keep the cursor line visible \
         even when an Unwitnessed plugin emits Hide over it"
    );
}

// ---------------------------------------------------------------------------
// RFC-107a — UniversalRevealState pre-algebra filter tests
//
// When `state.config.universal_reveal_state.is_revealed()` is true,
// `collect_tagged_display_directives` drops every directive whose
// `is_destructive()` is true *before* the algebra normalizes them.
// Pre-algebra positioning is the key invariant: decorations that would
// have been displaced by a destructive winner survive on reveal.
// ---------------------------------------------------------------------------

struct WitnessedStyleLinePlugin {
    priority: i16,
}

impl Plugin for WitnessedStyleLinePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId::from("witnessed-style-line")
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.on_display_safe(|_state, _app| {
            vec![
                kasane_core::plugin::algebra::safe_directive::SafeDisplayDirective::style_inline(
                    0,
                    0..5,
                    kasane_core::protocol::Style::default(),
                ),
            ]
        });
        r.declare_display_priority(self.priority);
    }
}

#[test]
fn universal_reveal_off_by_default_lets_hide_take_effect() {
    let mut registry = PluginRuntime::new();
    registry.register(UnwitnessedHidePlugin);

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![], vec![]].into();
    state.observed.cursor_pos.line = 0; // outside Hide range

    assert!(!state.config.universal_reveal_state.is_revealed());

    let directives = registry
        .view()
        .collect_display_directives(&AppView::new(&state));
    assert!(
        directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::Hide { .. })),
        "Hide must take effect when reveal is off; got {:?}",
        directives
    );
}

#[test]
fn universal_reveal_on_drops_full_hide() {
    let mut registry = PluginRuntime::new();
    registry.register(UnwitnessedHidePlugin);

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![], vec![]].into();
    state.config.universal_reveal_state.set(true);

    let directives = registry
        .view()
        .collect_display_directives(&AppView::new(&state));
    assert!(
        !directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::Hide { .. })),
        "Hide must be filtered when reveal is on; got {:?}",
        directives
    );
}

#[test]
fn universal_reveal_on_drops_hide_inline() {
    let mut registry = PluginRuntime::new();
    registry.register(UnwitnessedHideInlinePlugin);

    let mut state = AppState::default();
    state.observed.lines = vec![vec![Atom::plain("hello world")], vec![]].into();
    state.config.universal_reveal_state.set(true);

    let directives = registry
        .view()
        .collect_display_directives(&AppView::new(&state));
    assert!(
        !directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::HideInline { .. })),
        "HideInline must be filtered when reveal is on; got {:?}",
        directives
    );
}

#[test]
fn universal_reveal_on_drops_overlapping_hide_from_multiple_plugins() {
    let mut registry = PluginRuntime::new();
    registry.register(UnwitnessedHidePlugin); // Hide { range: 1..3 }
    // Second plugin emitting overlapping Hide on a separate range.
    struct SecondHidePlugin;
    impl Plugin for SecondHidePlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("second-hide-probe")
        }
        fn register(&self, r: &mut HandlerRegistry<()>) {
            r.on_display(|_state, _app| vec![DisplayDirective::Hide { range: 2..4 }]);
        }
    }
    registry.register(SecondHidePlugin);

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![], vec![], vec![]].into();
    state.config.universal_reveal_state.set(true);

    let directives = registry
        .view()
        .collect_display_directives(&AppView::new(&state));
    assert!(
        !directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::Hide { .. })),
        "All overlapping Hides from both plugins must be revealed by single toggle; got {:?}",
        directives
    );
}

#[test]
fn universal_reveal_pre_algebra_preserves_decoration_displaced_by_hide() {
    // The defining property of pre-algebra filtering:
    // A low-priority decoration on a range that would have been displaced
    // by a high-priority Hide remains visible on reveal.

    let mut registry = PluginRuntime::new();
    registry.register(WitnessedStyleLinePlugin { priority: 0 }); // StyleInline 0..5
    struct HighPriorityHidePlugin;
    impl Plugin for HighPriorityHidePlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from("high-prio-hide")
        }
        fn register(&self, r: &mut HandlerRegistry<()>) {
            r.on_display(|_state, _app| vec![DisplayDirective::Hide { range: 0..1 }]);
            r.declare_display_priority(100);
        }
    }
    registry.register(HighPriorityHidePlugin);

    let mut state = AppState::default();
    state.observed.lines = vec![vec![Atom::plain("hello")], vec![]].into();
    state.observed.cursor_pos.line = 1; // off the Hide line

    // With reveal OFF: Hide wins, decoration is displaced.
    let with_hide = registry
        .view()
        .collect_display_directives(&AppView::new(&state));
    let hide_present = with_hide
        .iter()
        .any(|d| matches!(d, DisplayDirective::Hide { .. }));
    assert!(hide_present, "Hide must be present when reveal is off");

    // With reveal ON: Hide is dropped pre-algebra, decoration survives.
    state.config.universal_reveal_state.set(true);
    let revealed = registry
        .view()
        .collect_display_directives(&AppView::new(&state));
    assert!(
        !revealed
            .iter()
            .any(|d| matches!(d, DisplayDirective::Hide { .. })),
        "Hide must be filtered on reveal"
    );
    assert!(
        revealed
            .iter()
            .any(|d| matches!(d, DisplayDirective::StyleInline { line: 0, .. })),
        "StyleInline that would have been displaced must survive on reveal (pre-algebra property); got {:?}",
        revealed
    );
}

#[test]
fn universal_reveal_toggle_is_idempotent() {
    let mut state = AppState::default();
    let initial = state.config.universal_reveal_state.is_revealed();
    state.config.universal_reveal_state.toggle();
    state.config.universal_reveal_state.toggle();
    assert_eq!(
        state.config.universal_reveal_state.is_revealed(),
        initial,
        "two toggles must restore the original state"
    );
}
