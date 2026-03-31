use crate::layout::Rect;
use crate::plugin::{
    FaceMerge, OrnamentModality, SourcedOrnamentBatch, SurfaceOrnAnchor, SurfaceOrnKind,
};
use crate::protocol::Face;
use crate::render::grid::CellGrid;
use crate::surface::{SurfaceId, SurfaceRegistry};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedSurfaceOrn {
    pub surface_id: Option<SurfaceId>,
    pub rect: Rect,
    pub kind: SurfaceOrnKind,
    pub face: Face,
}

#[derive(Debug, Clone)]
struct SurfaceCandidate {
    score: (i8, i16),
    resolved: ResolvedSurfaceOrn,
}

fn modality_rank(modality: OrnamentModality) -> i8 {
    match modality {
        OrnamentModality::Must => 2,
        OrnamentModality::Approximate => 1,
        OrnamentModality::May => 0,
    }
}

fn kind_order(kind: SurfaceOrnKind) -> u8 {
    match kind {
        SurfaceOrnKind::InactiveTint => 0,
        SurfaceOrnKind::FocusFrame => 1,
    }
}

fn upsert_surface_candidate(winners: &mut Vec<SurfaceCandidate>, candidate: SurfaceCandidate) {
    if let Some(existing) = winners.iter_mut().find(|winner| {
        winner.resolved.surface_id == candidate.resolved.surface_id
            && winner.resolved.kind == candidate.resolved.kind
    }) {
        if candidate.score > existing.score {
            *existing = candidate;
        }
        return;
    }
    winners.push(candidate);
}

pub(crate) fn resolve_surface_ornaments(
    batches: &[SourcedOrnamentBatch],
    surface_registry: Option<&SurfaceRegistry>,
    focused_pane_rect: Option<Rect>,
    total: Rect,
) -> Vec<ResolvedSurfaceOrn> {
    let workspace_rects =
        surface_registry.map(|registry| registry.workspace().compute_rects(total));
    let focused_surface_id = surface_registry.map(|registry| registry.workspace().focused());
    let focused_surface_rect = focused_pane_rect.or_else(|| {
        workspace_rects.as_ref().and_then(|rects| {
            focused_surface_id.and_then(|surface_id| rects.get(&surface_id).copied())
        })
    });

    let mut winners = Vec::new();
    for sourced in batches {
        for orn in &sourced.batch.surfaces {
            let score = (modality_rank(orn.modality), orn.priority);
            let resolved = match &orn.anchor {
                SurfaceOrnAnchor::FocusedSurface => {
                    let Some(rect) = focused_surface_rect else {
                        continue;
                    };
                    if orn.kind != SurfaceOrnKind::FocusFrame {
                        continue;
                    }
                    ResolvedSurfaceOrn {
                        surface_id: focused_surface_id,
                        rect,
                        kind: orn.kind,
                        face: orn.face,
                    }
                }
                SurfaceOrnAnchor::SurfaceKey(surface_key) => {
                    let Some(registry) = surface_registry else {
                        continue;
                    };
                    let Some(surface_id) = registry.surface_id_by_key(surface_key) else {
                        continue;
                    };
                    let Some(rect) = workspace_rects
                        .as_ref()
                        .and_then(|rects| rects.get(&surface_id).copied())
                    else {
                        continue;
                    };
                    let is_focused = Some(surface_id) == focused_surface_id;
                    match orn.kind {
                        SurfaceOrnKind::FocusFrame if !is_focused => continue,
                        SurfaceOrnKind::InactiveTint if is_focused => continue,
                        _ => {}
                    }
                    ResolvedSurfaceOrn {
                        surface_id: Some(surface_id),
                        rect,
                        kind: orn.kind,
                        face: orn.face,
                    }
                }
            };
            upsert_surface_candidate(&mut winners, SurfaceCandidate { score, resolved });
        }
    }

    let mut resolved: Vec<_> = winners.into_iter().map(|winner| winner.resolved).collect();
    resolved.sort_by_key(|item| {
        (
            kind_order(item.kind),
            item.surface_id.map(|id| id.0).unwrap_or(u32::MAX),
        )
    });
    resolved
}

pub(crate) fn apply_surface_ornaments_tui(grid: &mut CellGrid, ornaments: &[ResolvedSurfaceOrn]) {
    for orn in ornaments {
        match orn.kind {
            SurfaceOrnKind::InactiveTint => {
                apply_rect_face(grid, &orn.rect, &orn.face, FaceMerge::Background)
            }
            SurfaceOrnKind::FocusFrame => {
                apply_rect_perimeter_face(grid, &orn.rect, &orn.face, FaceMerge::Overlay)
            }
        }
    }
}

fn apply_rect_face(grid: &mut CellGrid, rect: &Rect, face: &Face, merge: FaceMerge) {
    let x_end = rect.x.saturating_add(rect.w).min(grid.width());
    let y_end = rect.y.saturating_add(rect.h).min(grid.height());
    for y in rect.y..y_end {
        for x in rect.x..x_end {
            if let Some(cell) = grid.get_mut(x, y) {
                merge.apply(&mut cell.face, face);
            }
        }
    }
}

fn apply_rect_perimeter_face(grid: &mut CellGrid, rect: &Rect, face: &Face, merge: FaceMerge) {
    let x_end = rect.x.saturating_add(rect.w).min(grid.width());
    let y_end = rect.y.saturating_add(rect.h).min(grid.height());
    if rect.w == 0 || rect.h == 0 || rect.x >= x_end || rect.y >= y_end {
        return;
    }

    for y in rect.y..y_end {
        for x in rect.x..x_end {
            let is_perimeter = x == rect.x || x + 1 == x_end || y == rect.y || y + 1 == y_end;
            if !is_perimeter {
                continue;
            }
            if let Some(cell) = grid.get_mut(x, y) {
                merge.apply(&mut cell.face, face);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SplitDirection;
    use crate::plugin::{OrnamentBatch, SurfaceOrn};
    use crate::protocol::{Color, NamedColor};
    use crate::surface::SurfaceRegistry;
    use crate::surface::buffer::KakouneBufferSurface;
    use crate::surface::status::StatusBarSurface;
    use crate::test_support::TestSurfaceBuilder;

    fn face(bg: NamedColor) -> Face {
        Face {
            bg: Color::Named(bg),
            ..Face::default()
        }
    }

    #[test]
    fn resolve_surface_ornaments_prefers_higher_priority_per_surface_and_kind() {
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let mut registry = SurfaceRegistry::new();
        registry
            .try_register(Box::new(KakouneBufferSurface::new()))
            .unwrap();
        registry
            .try_register(Box::new(StatusBarSurface::new()))
            .unwrap();
        registry
            .try_register(
                TestSurfaceBuilder::new(SurfaceId(200))
                    .key("test.right")
                    .build(),
            )
            .unwrap();
        registry.workspace_mut().root_mut().split(
            SurfaceId::BUFFER,
            SplitDirection::Vertical,
            0.5,
            SurfaceId(200),
        );
        registry.workspace_mut().focus(SurfaceId::BUFFER);

        let batches = vec![
            SourcedOrnamentBatch {
                plugin_id: crate::plugin::PluginId("low".into()),
                batch: OrnamentBatch {
                    surfaces: vec![SurfaceOrn {
                        anchor: SurfaceOrnAnchor::FocusedSurface,
                        kind: SurfaceOrnKind::FocusFrame,
                        face: face(NamedColor::Blue),
                        priority: 1,
                        modality: OrnamentModality::Approximate,
                    }],
                    ..OrnamentBatch::default()
                },
            },
            SourcedOrnamentBatch {
                plugin_id: crate::plugin::PluginId("high".into()),
                batch: OrnamentBatch {
                    surfaces: vec![
                        SurfaceOrn {
                            anchor: SurfaceOrnAnchor::FocusedSurface,
                            kind: SurfaceOrnKind::FocusFrame,
                            face: face(NamedColor::Red),
                            priority: 5,
                            modality: OrnamentModality::Must,
                        },
                        SurfaceOrn {
                            anchor: SurfaceOrnAnchor::SurfaceKey("test.right".into()),
                            kind: SurfaceOrnKind::InactiveTint,
                            face: face(NamedColor::Yellow),
                            priority: 2,
                            modality: OrnamentModality::Approximate,
                        },
                    ],
                    ..OrnamentBatch::default()
                },
            },
        ];

        let resolved = resolve_surface_ornaments(&batches, Some(&registry), None, total);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].kind, SurfaceOrnKind::InactiveTint);
        assert_eq!(resolved[1].kind, SurfaceOrnKind::FocusFrame);
        assert_eq!(resolved[1].face.bg, Color::Named(NamedColor::Red));
        assert_eq!(resolved[1].surface_id, Some(SurfaceId::BUFFER));
        assert_eq!(resolved[0].surface_id, Some(SurfaceId(200)));
    }

    #[test]
    fn resolve_surface_ornaments_enforces_focus_truth() {
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let mut registry = SurfaceRegistry::new();
        registry
            .try_register(Box::new(KakouneBufferSurface::new()))
            .unwrap();
        registry
            .try_register(Box::new(StatusBarSurface::new()))
            .unwrap();
        registry
            .try_register(
                TestSurfaceBuilder::new(SurfaceId(200))
                    .key("test.right")
                    .build(),
            )
            .unwrap();
        registry.workspace_mut().root_mut().split(
            SurfaceId::BUFFER,
            SplitDirection::Vertical,
            0.5,
            SurfaceId(200),
        );
        registry.workspace_mut().focus(SurfaceId(200));

        let batches = vec![SourcedOrnamentBatch {
            plugin_id: crate::plugin::PluginId("surface".into()),
            batch: OrnamentBatch {
                surfaces: vec![
                    SurfaceOrn {
                        anchor: SurfaceOrnAnchor::FocusedSurface,
                        kind: SurfaceOrnKind::InactiveTint,
                        face: face(NamedColor::Yellow),
                        priority: 1,
                        modality: OrnamentModality::Approximate,
                    },
                    SurfaceOrn {
                        anchor: SurfaceOrnAnchor::SurfaceKey("kasane.buffer".into()),
                        kind: SurfaceOrnKind::FocusFrame,
                        face: face(NamedColor::Blue),
                        priority: 1,
                        modality: OrnamentModality::Approximate,
                    },
                    SurfaceOrn {
                        anchor: SurfaceOrnAnchor::SurfaceKey("kasane.buffer".into()),
                        kind: SurfaceOrnKind::InactiveTint,
                        face: face(NamedColor::Red),
                        priority: 1,
                        modality: OrnamentModality::Approximate,
                    },
                ],
                ..OrnamentBatch::default()
            },
        }];

        let resolved = resolve_surface_ornaments(&batches, Some(&registry), None, total);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].surface_id, Some(SurfaceId::BUFFER));
        assert_eq!(resolved[0].kind, SurfaceOrnKind::InactiveTint);
    }

    #[test]
    fn apply_surface_ornaments_tui_tints_interior_and_frames_perimeter() {
        let mut grid = CellGrid::new(6, 4);
        let default_face = Face {
            bg: Color::Named(NamedColor::Black),
            ..Face::default()
        };
        grid.clear(&default_face);

        let ornaments = vec![
            ResolvedSurfaceOrn {
                surface_id: Some(SurfaceId(1)),
                rect: Rect {
                    x: 1,
                    y: 1,
                    w: 3,
                    h: 2,
                },
                kind: SurfaceOrnKind::InactiveTint,
                face: face(NamedColor::Blue),
            },
            ResolvedSurfaceOrn {
                surface_id: Some(SurfaceId(2)),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 5,
                    h: 4,
                },
                kind: SurfaceOrnKind::FocusFrame,
                face: face(NamedColor::Red),
            },
        ];

        apply_surface_ornaments_tui(&mut grid, &ornaments);

        assert_eq!(
            grid.get(2, 2).unwrap().face.bg,
            Color::Named(NamedColor::Blue)
        );
        assert_eq!(
            grid.get(0, 0).unwrap().face.bg,
            Color::Named(NamedColor::Red)
        );
        assert_eq!(
            grid.get(4, 3).unwrap().face.bg,
            Color::Named(NamedColor::Red)
        );
        assert_eq!(
            grid.get(5, 3).unwrap().face.bg,
            Color::Named(NamedColor::Black)
        );
    }
}
