//! Display Unit Model — operable units within display-transformed content.
//!
//! Display Units are the formal domain of the inverse projection ρ. They give the
//! input system a well-defined structure for dispatching events to display-transformed
//! content (fold summaries, hidden regions).
//!
//! A `DisplayUnitMap` is built from a non-identity `DisplayMap` and provides O(1)
//! lookup by display line. When `DisplayMap::is_identity()`, no `DisplayUnitMap` is
//! constructed (T5-DU: zero overhead when no display transforms are active).

use std::hash::{Hash, Hasher};
use std::ops::Range;

use crate::display::{DisplayLine, DisplayMap, InteractionPolicy, SourceMapping};
use crate::element::PluginTag;

/// Stable identity for a display unit, derived from content (not insertion order).
///
/// Content-addressed: same `(source, role)` pair always produces the same ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayUnitId(u64);

impl DisplayUnitId {
    /// Create a content-addressed ID from source mapping and semantic role.
    pub fn from_content(source: &UnitSource, role: &SemanticRole) -> Self {
        let mut hasher = std::hash::DefaultHasher::new();
        source.hash(&mut hasher);
        role.hash(&mut hasher);
        DisplayUnitId(hasher.finish())
    }
}

/// What this unit represents in the display.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SemanticRole {
    /// Normal buffer content (1:1 with a buffer line).
    BufferContent,
    /// Fold summary representing a collapsed range.
    FoldSummary,
    /// Plugin-defined role. Core applies Skip default; plugins override.
    Plugin(PluginTag, u32),
}

/// Classification of source mapping strength (σ).
///
/// Determines the default `InteractionPolicy` for a display unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStrength {
    /// Complete inverse exists (`Line`).
    Strong,
    /// Many-to-one mapping (`LineRange`).
    Weak,
    /// Sub-line only (`Span`).
    Partial,
}

/// Source mapping from a display unit to buffer coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnitSource {
    /// Single buffer line (σ strength: Strong).
    Line(usize),
    /// Multiple buffer lines — fold (σ strength: Weak).
    LineRange(Range<usize>),
    /// Sub-line range within a buffer line (σ strength: Partial). Future extension.
    Span {
        line: usize,
        byte_range: Range<usize>,
    },
}

impl UnitSource {
    /// Classify the σ strength of this source mapping.
    pub fn strength(&self) -> SourceStrength {
        match self {
            UnitSource::Line(_) => SourceStrength::Strong,
            UnitSource::LineRange(_) => SourceStrength::Weak,
            UnitSource::Span { .. } => SourceStrength::Partial,
        }
    }
}

/// Operable unit within the display-transformed UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayUnit {
    pub id: DisplayUnitId,
    pub display_line: usize,
    pub role: SemanticRole,
    pub source: UnitSource,
    pub interaction: InteractionPolicy,
}

/// Query interface for display units. Built from a non-identity `DisplayMap`.
///
/// In the initial implementation, each display line maps to exactly one `DisplayUnit`
/// (1:1 with `DisplayEntry`). Sub-line units are a future extension point.
#[derive(Debug, Clone)]
pub struct DisplayUnitMap {
    units: Vec<DisplayUnit>,
    /// display_line → unit index (1:1 in DU-1).
    line_to_unit: Vec<usize>,
}

impl DisplayUnitMap {
    /// Build a `DisplayUnitMap` from a non-identity `DisplayMap`.
    ///
    /// # Panics (debug only)
    ///
    /// Panics if `display_map.is_identity()`. Callers must fast-path identity maps
    /// by not constructing a `DisplayUnitMap` at all (T5-DU).
    pub fn build(display_map: &DisplayMap) -> Self {
        debug_assert!(
            !display_map.is_identity(),
            "DisplayUnitMap::build() must not be called on identity maps"
        );

        let count = display_map.display_line_count();
        let mut units = Vec::with_capacity(count);
        let mut line_to_unit = Vec::with_capacity(count);

        for dl in 0..count {
            let entry = display_map
                .entry(DisplayLine(dl))
                .expect("display line in range during build");

            let (source, role) = match entry.source() {
                SourceMapping::BufferLine(line) => {
                    (UnitSource::Line(line.0), SemanticRole::BufferContent)
                }
                SourceMapping::LineRange(range) => (
                    UnitSource::LineRange(range.clone()),
                    SemanticRole::FoldSummary,
                ),
            };

            let id = DisplayUnitId::from_content(&source, &role);
            let unit_idx = units.len();

            units.push(DisplayUnit {
                id,
                display_line: dl,
                role,
                source,
                interaction: entry.interaction(),
            });
            line_to_unit.push(unit_idx);
        }

        let dum = DisplayUnitMap {
            units,
            line_to_unit,
        };
        dum.check_invariants(display_map);
        dum
    }

    /// Number of display units.
    pub fn unit_count(&self) -> usize {
        self.units.len()
    }

    /// Get the display unit at the given display line (O(1)).
    pub fn unit_at_line(&self, display_line: usize) -> Option<&DisplayUnit> {
        self.line_to_unit
            .get(display_line)
            .and_then(|&idx| self.units.get(idx))
    }

    /// Hit test: find the display unit at the given mouse event coordinates.
    ///
    /// Translates `event_line` (screen-relative y from `MouseEvent.line`) to a
    /// display line using `display_scroll_offset`, then returns the unit at that
    /// line.  Returns `None` if the computed display line is out of range.
    pub fn hit_test(&self, event_line: u32, display_scroll_offset: usize) -> Option<&DisplayUnit> {
        let display_line = event_line as usize + display_scroll_offset;
        self.unit_at_line(display_line)
    }

    /// Get a display unit by index (O(1)).
    pub fn unit(&self, index: usize) -> Option<&DisplayUnit> {
        self.units.get(index)
    }

    /// Iterate over all display units in display order.
    pub fn iter(&self) -> impl Iterator<Item = &DisplayUnit> {
        self.units.iter()
    }

    /// Navigate from a display line in the given direction, using a custom
    /// policy function to determine how each unit should be treated.
    ///
    /// Returns the target unit (`Normal` or `Boundary`), or `None` if no
    /// suitable unit exists in the given direction.
    pub fn navigate_with_policy(
        &self,
        from_display_line: usize,
        direction: super::NavigationDirection,
        policy_fn: impl Fn(&DisplayUnit) -> super::NavigationPolicy,
    ) -> Option<&DisplayUnit> {
        use super::navigation::{NavigationDirection, NavigationPolicy};

        let indices: Box<dyn Iterator<Item = usize>> = match direction {
            NavigationDirection::Down => Box::new((from_display_line + 1)..self.units.len()),
            NavigationDirection::Up => Box::new((0..from_display_line).rev()),
        };
        for idx in indices {
            let unit = &self.units[idx];
            match policy_fn(unit) {
                NavigationPolicy::Normal | NavigationPolicy::Boundary { .. } => {
                    return Some(unit);
                }
                NavigationPolicy::Skip => continue,
            }
        }
        None
    }

    /// Navigate from a display line in the given direction, skipping `Skip`
    /// units and stopping at `Boundary` units.
    ///
    /// Uses the default navigation policy for each unit's semantic role.
    /// Returns the target unit (`Normal` or `Boundary`), or `None` if no
    /// suitable unit exists in the given direction.
    pub fn navigate(
        &self,
        from_display_line: usize,
        direction: super::NavigationDirection,
    ) -> Option<&DisplayUnit> {
        self.navigate_with_policy(from_display_line, direction, |u| {
            super::NavigationPolicy::default_for(&u.role)
        })
    }

    /// Verify DU-INV-1 through DU-INV-4 in debug builds.
    fn check_invariants(&self, display_map: &DisplayMap) {
        #[cfg(debug_assertions)]
        {
            let dl_count = display_map.display_line_count();

            // DU-INV-1 (Completeness): line_to_unit covers every display line.
            debug_assert_eq!(
                self.line_to_unit.len(),
                dl_count,
                "DU-INV-1: line_to_unit.len() ({}) != display_line_count ({})",
                self.line_to_unit.len(),
                dl_count,
            );
            for (dl, &idx) in self.line_to_unit.iter().enumerate() {
                debug_assert!(
                    idx < self.units.len(),
                    "DU-INV-1: line_to_unit[{dl}] = {idx} out of units range ({})",
                    self.units.len()
                );
                debug_assert_eq!(
                    self.units[idx].display_line, dl,
                    "DU-INV-1: units[{idx}].display_line ({}) != {dl}",
                    self.units[idx].display_line,
                );
            }

            // DU-INV-2 (Source Consistency): each unit's source matches its DisplayEntry.
            for unit in &self.units {
                let entry = display_map
                    .entry(DisplayLine(unit.display_line))
                    .expect("DU-INV-2: display_line out of range");
                let source_matches = match (&unit.source, &entry.source) {
                    (UnitSource::Line(l), SourceMapping::BufferLine(bl)) => *l == bl.0,
                    (UnitSource::LineRange(r), SourceMapping::LineRange(er)) => r == er,
                    _ => false,
                };
                debug_assert!(
                    source_matches,
                    "DU-INV-2: unit at display_line {} source mismatch",
                    unit.display_line,
                );
                debug_assert_eq!(
                    unit.interaction, entry.interaction,
                    "DU-INV-2: unit at display_line {} interaction mismatch",
                    unit.display_line,
                );
            }

            // DU-INV-3 (Order Preservation): buffer-origin units monotonically increase.
            let mut prev_buf: Option<usize> = None;
            for unit in &self.units {
                let buf_start = match &unit.source {
                    UnitSource::Line(l) => Some(*l),
                    UnitSource::LineRange(r) => Some(r.start),
                    _ => None,
                };
                if let Some(start) = buf_start {
                    if let Some(p) = prev_buf {
                        debug_assert!(
                            start >= p,
                            "DU-INV-3: non-monotonic: unit at display_line {} has buffer start {} after {}",
                            unit.display_line,
                            start,
                            p,
                        );
                    }
                    prev_buf = match &unit.source {
                        UnitSource::LineRange(r) => r.end.checked_sub(1),
                        _ => Some(start),
                    };
                }
            }

            // DU-INV-4 (Policy Soundness): σ strength constrains interaction policy.
            for unit in &self.units {
                match unit.source.strength() {
                    SourceStrength::Weak => {
                        debug_assert_eq!(
                            unit.interaction,
                            InteractionPolicy::ReadOnly,
                            "DU-INV-4: Weak source at display_line {} must have ReadOnly interaction",
                            unit.display_line,
                        );
                    }
                    SourceStrength::Strong => {
                        debug_assert_eq!(
                            unit.interaction,
                            InteractionPolicy::Normal,
                            "DU-INV-4: Strong source at display_line {} must have Normal interaction",
                            unit.display_line,
                        );
                    }
                    SourceStrength::Partial => {
                        // Partial sources are not yet produced by the builder.
                    }
                }
            }
        }
        // Suppress unused variable warning in release builds.
        let _ = display_map;
    }
}

/// Test-only invariant checker (non-debug gated).
///
/// Follows the `assert_display_map_invariants` pattern from `display/mod.rs`.
#[cfg(test)]
pub(crate) fn assert_display_unit_map_invariants(dum: &DisplayUnitMap, display_map: &DisplayMap) {
    let dl_count = display_map.display_line_count();

    // DU-INV-1 (Completeness)
    assert_eq!(
        dum.line_to_unit.len(),
        dl_count,
        "DU-INV-1: line_to_unit.len() != display_line_count"
    );
    for (dl, &idx) in dum.line_to_unit.iter().enumerate() {
        assert!(
            idx < dum.units.len(),
            "DU-INV-1: line_to_unit[{dl}] out of range"
        );
        assert_eq!(
            dum.units[idx].display_line, dl,
            "DU-INV-1: units[{idx}].display_line != {dl}"
        );
    }

    // DU-INV-2 (Source Consistency)
    for unit in &dum.units {
        let entry = display_map
            .entry(DisplayLine(unit.display_line))
            .expect("DU-INV-2: display_line out of range");
        let source_matches = match (&unit.source, &entry.source) {
            (UnitSource::Line(l), SourceMapping::BufferLine(bl)) => *l == bl.0,
            (UnitSource::LineRange(r), SourceMapping::LineRange(er)) => r == er,
            _ => false,
        };
        assert!(
            source_matches,
            "DU-INV-2: unit at display_line {} source mismatch",
            unit.display_line,
        );
        assert_eq!(
            unit.interaction, entry.interaction,
            "DU-INV-2: unit at display_line {} interaction mismatch",
            unit.display_line,
        );
    }

    // DU-INV-3 (Order Preservation)
    let mut prev_buf: Option<usize> = None;
    for unit in &dum.units {
        let buf_start = match &unit.source {
            UnitSource::Line(l) => Some(*l),
            UnitSource::LineRange(r) => Some(r.start),
            _ => None,
        };
        if let Some(start) = buf_start {
            if let Some(p) = prev_buf {
                assert!(
                    start >= p,
                    "DU-INV-3: non-monotonic at display_line {}",
                    unit.display_line,
                );
            }
            prev_buf = match &unit.source {
                UnitSource::LineRange(r) => r.end.checked_sub(1),
                _ => Some(start),
            };
        }
    }

    // DU-INV-4 (Policy Soundness)
    for unit in &dum.units {
        match unit.source.strength() {
            SourceStrength::Weak => {
                assert_eq!(
                    unit.interaction,
                    InteractionPolicy::ReadOnly,
                    "DU-INV-4: Weak source at display_line {} must be ReadOnly",
                    unit.display_line,
                );
            }
            SourceStrength::Strong => {
                assert_eq!(
                    unit.interaction,
                    InteractionPolicy::Normal,
                    "DU-INV-4: Strong source at display_line {} must be Normal",
                    unit.display_line,
                );
            }
            SourceStrength::Partial => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::display::resolve::{self, DirectiveSet};
    use crate::display::{DisplayDirective, DisplayMap};
    use crate::plugin::PluginId;
    use crate::protocol::{Atom, Face};

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "must not be called on identity maps")]
    fn build_panics_on_identity_map() {
        let dm = DisplayMap::identity(5);
        let _ = DisplayUnitMap::build(&dm);
    }

    #[test]
    fn fold_produces_fold_summary_unit() {
        let directives = vec![DisplayDirective::Fold {
            range: 2..5,
            summary: vec![Atom {
                face: Face::default(),
                contents: "folded".into(),
            }],
        }];
        let dm = DisplayMap::build(8, &directives);
        let dum = DisplayUnitMap::build(&dm);

        // Find the fold summary unit
        let fold_unit = dum.iter().find(|u| u.role == SemanticRole::FoldSummary);
        assert!(fold_unit.is_some(), "expected a FoldSummary unit");
        let fold_unit = fold_unit.unwrap();

        assert_eq!(fold_unit.source, UnitSource::LineRange(2..5));
        assert_eq!(fold_unit.interaction, InteractionPolicy::ReadOnly);
        assert_eq!(fold_unit.source.strength(), SourceStrength::Weak);

        assert_display_unit_map_invariants(&dum, &dm);
    }

    #[test]
    fn hide_removes_lines_from_unit_map() {
        let directives = vec![DisplayDirective::Hide { range: 1..3 }];
        let dm = DisplayMap::build(5, &directives);
        let dum = DisplayUnitMap::build(&dm);

        // 5 - 2 = 3 display lines → 3 units
        assert_eq!(dum.unit_count(), 3);

        // All remaining units should be BufferContent
        for unit in dum.iter() {
            assert_eq!(unit.role, SemanticRole::BufferContent);
        }

        // Verify correct line renumbering
        assert_eq!(dum.unit_at_line(0).unwrap().source, UnitSource::Line(0));
        assert_eq!(dum.unit_at_line(1).unwrap().source, UnitSource::Line(3));
        assert_eq!(dum.unit_at_line(2).unwrap().source, UnitSource::Line(4));

        assert_display_unit_map_invariants(&dum, &dm);
    }

    #[test]
    fn content_addressed_id_stability() {
        let directives = vec![DisplayDirective::Fold {
            range: 2..5,
            summary: vec![Atom {
                face: Face::default(),
                contents: "folded".into(),
            }],
        }];

        let dm1 = DisplayMap::build(8, &directives);
        let dum1 = DisplayUnitMap::build(&dm1);

        let dm2 = DisplayMap::build(8, &directives);
        let dum2 = DisplayUnitMap::build(&dm2);

        // Same directives → same IDs
        for (u1, u2) in dum1.iter().zip(dum2.iter()) {
            assert_eq!(u1.id, u2.id, "IDs should be stable across builds");
        }
    }

    #[test]
    fn different_sources_produce_different_ids() {
        let id_line =
            DisplayUnitId::from_content(&UnitSource::Line(5), &SemanticRole::BufferContent);
        let id_range =
            DisplayUnitId::from_content(&UnitSource::LineRange(5..10), &SemanticRole::FoldSummary);
        assert_ne!(id_line, id_range);
    }

    #[test]
    fn mixed_fold_and_hide() {
        let directives = vec![
            DisplayDirective::Fold {
                range: 1..3,
                summary: vec![Atom {
                    face: Face::default(),
                    contents: "fold".into(),
                }],
            },
            DisplayDirective::Hide { range: 5..7 },
        ];
        let dm = DisplayMap::build(10, &directives);
        let dum = DisplayUnitMap::build(&dm);

        assert_display_unit_map_invariants(&dum, &dm);

        // Verify basic structure
        assert_eq!(dum.unit_count(), dm.display_line_count());
    }

    #[test]
    fn source_strength_classification() {
        assert_eq!(UnitSource::Line(0).strength(), SourceStrength::Strong);
        assert_eq!(UnitSource::LineRange(0..5).strength(), SourceStrength::Weak);
        assert_eq!(
            UnitSource::Span {
                line: 0,
                byte_range: 0..10
            }
            .strength(),
            SourceStrength::Partial
        );
    }

    // --- hit_test ---

    /// Helper: build a DisplayUnitMap from a fold scenario.
    /// 8 buffer lines, fold 2..5.
    /// Display: [buf(0), buf(1), fold(2..5), buf(5), buf(6), buf(7)]
    ///           dl=0    dl=1    dl=2        dl=3    dl=4    dl=5
    fn build_fold_dum() -> (DisplayMap, DisplayUnitMap) {
        let directives = vec![DisplayDirective::Fold {
            range: 2..5,
            summary: vec![Atom {
                face: Face::default(),
                contents: "folded".into(),
            }],
        }];
        let dm = DisplayMap::build(8, &directives);
        let dum = DisplayUnitMap::build(&dm);
        (dm, dum)
    }

    #[test]
    fn hit_test_returns_buffer_content_unit() {
        let (_dm, dum) = build_fold_dum();
        // Display line 0 = buffer(0)
        let unit = dum.hit_test(0, 0).unwrap();
        assert_eq!(unit.role, SemanticRole::BufferContent);
        assert_eq!(unit.source, UnitSource::Line(0));
    }

    #[test]
    fn hit_test_returns_fold_summary_unit() {
        let (_dm, dum) = build_fold_dum();
        // Display line 2 = fold summary
        let unit = dum.hit_test(2, 0).unwrap();
        assert_eq!(unit.role, SemanticRole::FoldSummary);
        assert_eq!(unit.interaction, InteractionPolicy::ReadOnly);
    }

    #[test]
    fn hit_test_with_scroll_offset() {
        let (_dm, dum) = build_fold_dum();
        // event_line=0, offset=2 → display_line=2 = fold summary
        let unit = dum.hit_test(0, 2).unwrap();
        assert_eq!(unit.role, SemanticRole::FoldSummary);

        // event_line=1, offset=2 → display_line=3 = buf(5)
        let unit = dum.hit_test(1, 2).unwrap();
        assert_eq!(unit.role, SemanticRole::BufferContent);
        assert_eq!(unit.source, UnitSource::Line(5));
    }

    #[test]
    fn hit_test_out_of_range() {
        let (_dm, dum) = build_fold_dum();
        // Display has 6 lines; line 6 is out of range
        assert!(dum.hit_test(6, 0).is_none());
        // Large scroll offset pushes past end
        assert!(dum.hit_test(0, 100).is_none());
    }

    // --- navigate ---

    #[test]
    fn navigate_down_reaches_next_buffer_line() {
        let (_dm, dum) = build_fold_dum();
        // Display: [buf(0), buf(1), fold(2..5), buf(5), buf(6), buf(7)]
        //           dl=0    dl=1    dl=2        dl=3    dl=4    dl=5
        // From dl=0, navigate Down → buf(1) at dl=1
        let target = dum
            .navigate(0, crate::display::NavigationDirection::Down)
            .unwrap();
        assert_eq!(target.display_line, 1);
        assert_eq!(target.role, SemanticRole::BufferContent);
    }

    #[test]
    fn navigate_up_from_fold() {
        let (_dm, dum) = build_fold_dum();
        // From dl=2 (fold), navigate Up → buf(1) at dl=1
        let target = dum
            .navigate(2, crate::display::NavigationDirection::Up)
            .unwrap();
        assert_eq!(target.display_line, 1);
        assert_eq!(target.role, SemanticRole::BufferContent);
    }

    #[test]
    fn navigate_down_stops_at_boundary() {
        let (_dm, dum) = build_fold_dum();
        // From dl=1 (buf(1)), navigate Down → fold summary at dl=2 (Boundary)
        let target = dum
            .navigate(1, crate::display::NavigationDirection::Down)
            .unwrap();
        assert_eq!(target.display_line, 2);
        assert_eq!(target.role, SemanticRole::FoldSummary);
    }

    #[test]
    fn navigate_from_last_unit_down_returns_none() {
        let (_dm, dum) = build_fold_dum();
        // dl=5 is the last unit
        assert!(
            dum.navigate(5, crate::display::NavigationDirection::Down)
                .is_none()
        );
    }

    #[test]
    fn navigate_from_first_unit_up_returns_none() {
        let (_dm, dum) = build_fold_dum();
        assert!(
            dum.navigate(0, crate::display::NavigationDirection::Up)
                .is_none()
        );
    }

    // --- navigate_with_policy ---

    #[test]
    fn navigate_with_policy_custom_skip() {
        let (_dm, dum) = build_fold_dum();
        // Display: [buf(0), buf(1), fold(2..5), buf(5), buf(6), buf(7)]
        //           dl=0    dl=1    dl=2        dl=3    dl=4    dl=5
        // Custom policy: mark BufferContent as Skip (only Boundary/Normal are navigable)
        let target = dum.navigate_with_policy(0, crate::display::NavigationDirection::Down, |u| {
            match &u.role {
                SemanticRole::BufferContent => crate::display::NavigationPolicy::Skip,
                other => crate::display::NavigationPolicy::default_for(other),
            }
        });
        // Should skip buf(1) at dl=1, reach fold at dl=2 (Boundary)
        let target = target.unwrap();
        assert_eq!(target.display_line, 2);
        assert_eq!(target.role, SemanticRole::FoldSummary);
    }

    #[test]
    fn navigate_with_policy_matches_default() {
        let (_dm, dum) = build_fold_dum();
        // navigate_with_policy with default_for should produce the same result as navigate
        for start_line in 0..dum.unit_count() {
            for dir in [
                crate::display::NavigationDirection::Up,
                crate::display::NavigationDirection::Down,
            ] {
                let default_result = dum.navigate(start_line, dir);
                let policy_result = dum.navigate_with_policy(start_line, dir, |u| {
                    crate::display::NavigationPolicy::default_for(&u.role)
                });
                assert_eq!(
                    default_result.map(|u| u.display_line),
                    policy_result.map(|u| u.display_line),
                    "mismatch at start_line={start_line}, dir={dir:?}"
                );
            }
        }
    }

    // --- proptest ---

    fn arb_display_directive(max_line: usize) -> impl Strategy<Value = DisplayDirective> {
        let m = max_line.max(1);
        prop_oneof![
            (0usize..m, 1usize..m.min(8).max(1) + 1).prop_map(move |(s, len)| {
                DisplayDirective::Fold {
                    range: s..(s + len).min(m),
                    summary: vec![Atom {
                        face: Face::default(),
                        contents: "...".into(),
                    }],
                }
            }),
            (0usize..m, 1usize..m.min(8).max(1) + 1).prop_map(move |(s, len)| {
                DisplayDirective::Hide {
                    range: s..(s + len).min(m),
                }
            }),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn display_unit_invariants_hold(
            (line_count, directives) in (1usize..50).prop_flat_map(|lc| {
                (Just(lc), prop::collection::vec(arb_display_directive(lc), 0..8))
            })
        ) {
            let mut set = DirectiveSet::default();
            for (i, d) in directives.into_iter().enumerate() {
                set.push(d, 0, PluginId(format!("p{i}")));
            }
            let resolved = resolve::resolve(&set, line_count);
            let dm = DisplayMap::build(line_count, &resolved);
            if !dm.is_identity() {
                let dum = DisplayUnitMap::build(&dm);
                assert_display_unit_map_invariants(&dum, &dm);
            }
        }

        #[test]
        fn hit_test_in_range_returns_some(
            (line_count, directives) in (1usize..50).prop_flat_map(|lc| {
                (Just(lc), prop::collection::vec(arb_display_directive(lc), 1..8))
            })
        ) {
            let mut set = DirectiveSet::default();
            for (i, d) in directives.into_iter().enumerate() {
                set.push(d, 0, PluginId(format!("p{i}")));
            }
            let resolved = resolve::resolve(&set, line_count);
            let dm = DisplayMap::build(line_count, &resolved);
            if !dm.is_identity() {
                let dum = DisplayUnitMap::build(&dm);
                for dl in 0..dm.display_line_count() {
                    prop_assert!(
                        dum.hit_test(dl as u32, 0).is_some(),
                        "hit_test({dl}, 0) returned None on a {}-line map",
                        dm.display_line_count()
                    );
                }
                // One past the end should be None
                prop_assert!(dum.hit_test(dm.display_line_count() as u32, 0).is_none());
            }
        }

        #[test]
        fn content_addressed_ids_deterministic(
            (line_count, directives) in (1usize..50).prop_flat_map(|lc| {
                (Just(lc), prop::collection::vec(arb_display_directive(lc), 1..8))
            })
        ) {
            let mut set = DirectiveSet::default();
            for (i, d) in directives.iter().enumerate() {
                set.push(d.clone(), 0, PluginId(format!("p{i}")));
            }
            let resolved = resolve::resolve(&set, line_count);
            let dm = DisplayMap::build(line_count, &resolved);
            if !dm.is_identity() {
                let dum1 = DisplayUnitMap::build(&dm);
                let dum2 = DisplayUnitMap::build(&dm);
                for (u1, u2) in dum1.iter().zip(dum2.iter()) {
                    prop_assert_eq!(u1.id, u2.id);
                }
            }
        }
    }
}
